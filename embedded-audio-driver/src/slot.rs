use core::future::poll_fn;
use core::ops::{Deref, DerefMut};
use core::task::Poll;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU8, Ordering};

use embassy_sync::waitqueue::AtomicWaker;

// Represents the possible states of the slot.
// This enum is the core of the state machine that ensures safe access to the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum State {
    /// The slot is empty and ready for a producer (OutPort) to write to it.
    Empty,
    /// A consumer (InPort) has acquired the slot and is currently reading from the buffer.
    Reading,
    /// A producer (OutPort) has acquired the slot and is currently writing to the buffer.
    Writing,
    /// The slot is full of data and ready for a consumer (InPort) to read from it.
    Full,
}

// Allows converting State enum to a u8 for atomic operations.
impl From<State> for u8 {
    fn from(state: State) -> Self {
        state as u8
    }
}

// Allows converting a u8 back to a State, returning an error for invalid values.
impl TryFrom<u8> for State {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            x if x == State::Empty as u8 => Ok(State::Empty),
            x if x == State::Reading as u8 => Ok(State::Reading),
            x if x == State::Writing as u8 => Ok(State::Writing),
            x if x == State::Full as u8 => Ok(State::Full),
            _ => Err(()),
        }
    }
}

// Holds the shared state between the InPort and OutPort.
// This is what allows them to synchronize.
struct SharedState {
    // The current state of the slot, managed atomically.
    state: AtomicU8,
    // Waker for the task waiting on the InPort (consumer).
    in_port_waker: AtomicWaker,
    // Waker for the task waiting on the OutPort (producer).
    out_port_waker: AtomicWaker,
}

/// A single-slot channel for safely passing a mutable buffer between an asynchronous
/// producer and consumer without any allocations.
/// The lifetime 'b is tied to the buffer that is being managed.
pub struct Slot<'b> {
    // The buffer itself. `UnsafeCell` is needed because we give out a mutable reference
    // from a shared reference (&Slot). Access is safe thanks to our state machine.
    // `Option` allows the buffer to be "taken" by a payload and returned later.
    buffer: UnsafeCell<Option<&'b mut [u8]>>,
    shared: SharedState,
}

// This is safe because access to the UnsafeCell is externally synchronized
// by the atomic `state` variable.
unsafe impl<'b> Sync for Slot<'b> {}

impl<'b> Slot<'b> {
    /// Creates a new Slot, initially managing the provided buffer.
    /// The slot starts in the `Empty` state if a buffer is provided,
    /// or remains `Empty` if `None` is passed.
    pub fn new(buffer: Option<&'b mut [u8]>) -> Self {
        let initial_state = if buffer.is_some() {
            State::Empty as u8
        } else {
            // If no buffer, we can consider it full to prevent producer from writing
            // until a buffer is set and consumed. Or just keep it Empty. Let's stick with Empty.
            State::Empty as u8
        };
        Slot {
            buffer: UnsafeCell::new(buffer),
            shared: SharedState {
                state: AtomicU8::new(initial_state),
                in_port_waker: AtomicWaker::new(),
                out_port_waker: AtomicWaker::new(),
            },
        }
    }

    /// Allows replacing the buffer managed by the Slot.
    /// This should only be done when the slot is in a known state (e.g., Empty)
    /// and no operations are pending. The caller is responsible for ensuring safety.
    pub fn set_buffer(&mut self, buffer: Option<&'b mut [u8]>) {
        let new_state = if buffer.is_some() {
            State::Empty
        } else {
            // Or another appropriate state if needed.
            State::Empty
        };
        self.buffer = UnsafeCell::new(buffer);
        self.shared.state.store(new_state as u8, Ordering::Relaxed);
    }

    /// Splits the Slot into its two handles, the consumer (`InPort`) and producer (`OutPort`).
    /// The lifetime 'a ensures that the handles cannot outlive the Slot itself.
    pub fn split<'a>(&'a self) -> (InPort<'a, 'b>, OutPort<'a, 'b>) {
        (InPort { slot: self }, OutPort { slot: self })
    }
}

/// The consumer side of the channel (downstream).
/// It can asynchronously acquire the buffer to read data from it.
pub struct InPort<'a, 'b> {
    slot: &'a Slot<'b>,
}

impl<'a, 'b> InPort<'a, 'b> {
    /// Asynchronously waits until the slot is full and acquires the buffer for reading.
    /// Returns a guard `InPayload` which provides access to the data.
    pub async fn acquire(&self) -> InPayload<'a, 'b> {
        poll_fn(|cx| {
            // Atomically check if the state is `Full`. If it is, change it to `Reading`.
            // `Acquire` ordering ensures that we see the producer's write.
            // `Relaxed` for failure is fine as we will just retry or sleep.
            if self.slot.shared.state.compare_exchange(
                State::Full as u8,
                State::Reading as u8,
                Ordering::Acquire,
                Ordering::Relaxed,
            ).is_ok() {
                // State transition was successful. Now we can safely take the buffer.
                // SAFETY: The state machine guarantees we have exclusive access now.
                let buffer = unsafe { (*self.slot.buffer.get()).take() }
                    .expect("Bug: Slot was in Full state but buffer was None");
                
                // Wake the producer, in case it was waiting to know the buffer is free.
                self.slot.shared.out_port_waker.wake();

                Poll::Ready(InPayload { slot: self.slot, data: buffer })
            } else {
                // The slot is not full. Register our waker to be woken up later.
                self.slot.shared.in_port_waker.register(cx.waker());
                Poll::Pending
            }
        }).await
    }
}

/// The producer side of the channel (upstream).
/// It can asynchronously acquire the buffer to write data into it.
pub struct OutPort<'a, 'b> {
    slot: &'a Slot<'b>,
}

impl<'a, 'b> OutPort<'a, 'b> {
    /// Asynchronously waits until the slot is empty and acquires the buffer for writing.
    /// Returns a guard `OutPayload` which provides access to the data.
    pub async fn acquire(&self) -> OutPayload<'a, 'b> {
        poll_fn(|cx| {
            // Atomically check if the state is `Empty`. If it is, change it to `Writing`.
            if self.slot.shared.state.compare_exchange(
                State::Empty as u8,
                State::Writing as u8,
                Ordering::Acquire,
                Ordering::Relaxed,
            ).is_ok() {
                // State transition was successful. We can now safely take the buffer.
                // SAFETY: The state machine guarantees we have exclusive access now.
                let buffer = unsafe { (*self.slot.buffer.get()).take() }
                    .expect("Bug: Slot was in Empty state but buffer was None");
                
                // Wake the consumer, in case it was waiting to know it's being written to.
                // This is often not necessary but can be useful in some protocols.
                self.slot.shared.in_port_waker.wake();
                
                Poll::Ready(OutPayload { slot: self.slot, data: buffer })
            } else {
                // The slot is not empty. Register our waker to be woken up later.
                self.slot.shared.out_port_waker.register(cx.waker());
                Poll::Pending
            }
        }).await
    }
}

/// A RAII guard for the buffer when acquired by the consumer (`InPort`).
/// When this guard is dropped, it automatically returns the buffer to the slot,
/// sets the state to `Empty`, and wakes the producer task.
pub struct InPayload<'a, 'b> {
    slot: &'a Slot<'b>,
    data: &'b mut [u8],
}

impl<'a, 'b> Drop for InPayload<'a, 'b> {
    fn drop(&mut self) {
        // We must return the buffer to the slot.
        // We use a dummy slice to move out of `&mut self.data`.
        let dummy_slice = &mut [];
        let buffer = core::mem::replace(&mut self.data, dummy_slice);

        // SAFETY: We have exclusive access, so we can modify the UnsafeCell.
        unsafe {
            (*self.slot.buffer.get()).replace(buffer);
        }

        // After reading is done, the slot is now `Empty`.
        // `Release` ordering ensures our state change is visible to the producer.
        self.slot.shared.state.store(State::Empty as u8, Ordering::Release);
        
        // Wake up the producer task, as the slot is now empty and ready for writing.
        self.slot.shared.out_port_waker.wake();
    }
}

// Allow direct access to the underlying buffer data.
impl<'a, 'b> Deref for InPayload<'a, 'b> {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        self.data
    }
}

impl<'a, 'b> DerefMut for InPayload<'a, 'b> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data
    }
}


/// A RAII guard for the buffer when acquired by the producer (`OutPort`).
/// When this guard is dropped, it automatically returns the buffer to the slot,
/// sets the state to `Full`, and wakes the consumer task.
pub struct OutPayload<'a, 'b> {
    slot: &'a Slot<'b>,
    data: &'b mut [u8],
}

impl<'a, 'b> Drop for OutPayload<'a, 'b> {
    fn drop(&mut self) {
        // Return the buffer to the slot.
        let dummy_slice = &mut [];
        let buffer = core::mem::replace(&mut self.data, dummy_slice);

        // SAFETY: We have exclusive access.
        unsafe {
            (*self.slot.buffer.get()).replace(buffer);
        }

        // After writing is done, the slot is now `Full`.
        // `Release` ordering ensures our write and state change are visible to the consumer.
        self.slot.shared.state.store(State::Full as u8, Ordering::Release);
        
        // Wake up the consumer task, as the slot is now full and ready for reading.
        self.slot.shared.in_port_waker.wake();
    }
}

// Allow direct access to the underlying buffer data.
impl<'a, 'b> Deref for OutPayload<'a, 'b> {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        self.data
    }
}

impl<'a, 'b> DerefMut for OutPayload<'a, 'b> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data
    }
}