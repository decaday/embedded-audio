use core::future::poll_fn;
use core::ops::{Deref, DerefMut};
use core::task::Poll;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU8, Ordering};

use embassy_sync::waitqueue::AtomicWaker;

// This enum is the core of the state machine that ensures safe access to the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum State {
    /// The slot is empty and ready for a producer (SlotProducer) to write to it.
    Empty,
    /// A consumer (SlotConsumer) has acquired the slot and is currently reading from the buffer.
    Reading,
    /// A producer (SlotProducer) has acquired the slot and is currently writing to the buffer.
    Writing,
    /// The slot is full of data and ready for a consumer (SlotConsumer) to read from it.
    Full,
    /// This state is used when no buffer is set, preventing any operations.
    NoneBuffer,
}

impl From<State> for u8 {
    fn from(state: State) -> Self {
        state as u8
    }
}

impl TryFrom<u8> for State {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            x if x == State::Empty as u8 => Ok(State::Empty),
            x if x == State::Reading as u8 => Ok(State::Reading),
            x if x == State::Writing as u8 => Ok(State::Writing),
            x if x == State::Full as u8 => Ok(State::Full),
            x if x == State::NoneBuffer as u8 => Ok(State::NoneBuffer),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum PayloadMetadata {
    First,
    Last,
    Middle,
}

impl From<PayloadMetadata> for u8 {
    fn from(meta: PayloadMetadata) -> Self {
        meta as u8
    }
}

impl TryFrom<u8> for PayloadMetadata {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            x if x == PayloadMetadata::First as u8 => Ok(PayloadMetadata::First),
            x if x == PayloadMetadata::Last as u8 => Ok(PayloadMetadata::Last),
            x if x == PayloadMetadata::Middle as u8 => Ok(PayloadMetadata::Middle),
            _ => Err(()),
        }
    }
}

// Holds the shared state between the SlotConsumer and SlotProducer.
// allows them to synchronize.
struct SharedState {
    // The current state of the slot, managed atomically.
    state: AtomicU8,
    // Payload metadata
    payload_metadata: AtomicU8,
    // Waker for the task waiting on the SlotConsumer.
    consumer_waker: AtomicWaker,
    // Waker for the task waiting on the SlotProducer.
    producer_waker: AtomicWaker,
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
    /// The slot starts in the `Empty` state if a buffer is provided.
    pub fn new(buffer: Option<&'b mut [u8]>) -> Self {
        let initial_state = if buffer.is_some() {
            State::Empty
        } else {
            State::NoneBuffer
        };
        Slot {
            buffer: UnsafeCell::new(buffer),
            shared: SharedState {
                state: AtomicU8::new(initial_state as u8),
                consumer_waker: AtomicWaker::new(),
                producer_waker: AtomicWaker::new(),
                payload_metadata: AtomicU8::new(PayloadMetadata::Middle as u8),
            },
        }
    }

    /// Allows replacing the buffer managed by the Slot.
    /// This should only be done when the slot state is Empty.
    pub fn set_buffer(&mut self, buffer: Option<&'b mut [u8]>) {
        if self.shared.state.load(Ordering::Relaxed) != State::Empty as u8 {
            // TODO: Result
            panic!("Cannot set buffer when slot is not empty");
        }

        let new_state = if buffer.is_some() {
            State::Empty
        } else {
            State::NoneBuffer
        };
        self.buffer = UnsafeCell::new(buffer);
        self.shared.state.store(new_state as u8, Ordering::Relaxed);

        if new_state == State::Empty {
            self.shared.producer_waker.wake();
        }
    }

    /// Splits the Slot into its two handles, the `SlotConsumer` and `SlotProducer`.
    /// The lifetime 'a ensures that the handles cannot outlive the Slot itself.
    pub fn split<'a>(&'a self) -> (SlotProducer<'a, 'b>, SlotConsumer<'a, 'b>) {
        (SlotProducer { slot: self }, SlotConsumer { slot: self })
    }
}

/// The consumer side of the channel (downstream).
/// It can asynchronously acquire the buffer to read data from it.
pub struct SlotConsumer<'a, 'b> {
    slot: &'a Slot<'b>,
}

impl<'a, 'b> SlotConsumer<'a, 'b> {
    /// Asynchronously waits until the slot is full and acquires the buffer for reading.
    /// Returns a guard `Payload` which provides access to the data.
    pub async fn acquire(&self) -> Payload<'a, 'b> {
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
                // Wake the producer, in case it was waiting to know the buffer is free.
                self.slot.shared.producer_waker.wake();

                Poll::Ready(Payload::new_from_slot(self.slot))
            } else {
                // The slot is not full. Register our waker to be woken up later.
                self.slot.shared.consumer_waker.register(cx.waker());
                Poll::Pending
            }
        }).await
    }
}

/// The producer side of the channel (upstream).
/// It can asynchronously acquire the buffer to write data into it.
pub struct SlotProducer<'a, 'b> {
    slot: &'a Slot<'b>,
}

impl<'a, 'b> SlotProducer<'a, 'b> {
    /// Asynchronously waits until the slot is empty and acquires the buffer for writing.
    /// Returns a guard `Payload` which provides access to the data.
    pub async fn acquire(&self) -> Payload<'a, 'b> {
        poll_fn(|cx| {
            // Atomically check if the state is `Empty`. If it is, change it to `Writing`.
            if self.slot.shared.state.compare_exchange(
                State::Empty as u8,
                State::Writing as u8,
                Ordering::Acquire,
                Ordering::Relaxed,
            ).is_ok() {
                // Wake the consumer, in case it was waiting to know it's being written to.
                // This is often not necessary but can be useful in some protocols.
                self.slot.shared.consumer_waker.wake();
                
                Poll::Ready(Payload::new_from_slot(self.slot))
            } else {
                // The slot is not empty. Register our waker to be woken up later.
                self.slot.shared.producer_waker.register(cx.waker());
                Poll::Pending
            }
        }).await
    }
}

/// A RAII guard for the buffer when acquired by the producer (`SlotProducer`).
/// When this guard is dropped, it automatically returns the buffer to the slot,
/// sets the state to `Full`, and wakes the consumer task.
pub struct Payload<'a, 'b> {
    slot: &'a Slot<'b>,
    data: &'b mut [u8],
    metadata: PayloadMetadata,
    is_in: bool,
}

impl<'a, 'b> Payload<'a, 'b> {
    pub fn new_from_slot(slot: &'a Slot<'b>) -> Self {
        // State transition was successful. Now we can safely take the buffer.
        // SAFETY: The state machine guarantees we have exclusive access now.
        let buffer = unsafe { (*slot.buffer.get()).take() }
            .expect("Bug: Slot was in Full state but buffer was None");
        let is_in = slot.shared.state.load(Ordering::Acquire) == State::Full as u8;

        Payload {
            slot,
            data: buffer,
            metadata: PayloadMetadata::try_from(slot.shared.payload_metadata.load(Ordering::Acquire))
                .unwrap(),
            is_in,
        }
    }
}

impl<'a, 'b> Drop for Payload<'a, 'b> {
    fn drop(&mut self) {
        // Return the buffer to the slot.
        let dummy_slice = &mut [];
        let buffer = core::mem::replace(&mut self.data, dummy_slice);

        // SAFETY: We have exclusive access.
        unsafe {
            (*self.slot.buffer.get()).replace(buffer);
        }

        self.slot.shared.payload_metadata.store(self.metadata as u8, Ordering::Release);

        if self.is_in {
            // After reading is done, the slot is now `Empty`.
            // `Release` ordering ensures our state change is visible to the producer.
            self.slot.shared.state.store(State::Empty as u8, Ordering::Release);
            
            // Wake up the producer task, as the slot is now empty and ready for writing.
            self.slot.shared.producer_waker.wake();
        } else {
            self.slot.shared.state.store(State::Full as u8, Ordering::Release);
            
            // Wake up the consumer task, as the slot is now full and ready for reading.
            self.slot.shared.consumer_waker.wake();
        }
    }
}

// Allow direct access to the underlying buffer data.
impl<'a, 'b> Deref for Payload<'a, 'b> {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        self.data
    }
}

impl<'a, 'b> DerefMut for Payload<'a, 'b> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data
    }
}