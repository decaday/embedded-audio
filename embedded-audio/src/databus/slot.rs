use core::future::poll_fn;
use core::task::Poll;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU8, Ordering};

use embassy_sync::waitqueue::AtomicWaker;
use embedded_audio_driver::databus::Databus;
use embedded_audio_driver::payload::{Metadata, Payload};

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
    // Payload metadata
    payload_metadata: UnsafeCell<Option<Metadata>>,
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
            },
            payload_metadata: UnsafeCell::new(None),
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

    unsafe fn new_payload(&'b self, is_write: bool) -> Payload<'b, Self> {
        Payload::new(
            (*self.buffer.get()).take().unwrap(),
            (*self.payload_metadata.get()).take().unwrap_or_default(),
            is_write,
            self,
        )
    }

    pub fn get_current_metadata(&self) -> Option<Metadata> {
        // This is safe because we ensure the metadata is set before using it.
        unsafe { *self.payload_metadata.get() }.clone()
    }
}

impl<'b> Databus<'b> for Slot<'b> {
    async fn acquire_read(&'b self) -> Payload<'b, Self> {
        poll_fn(|cx| {
            // Atomically check if the state is `Full`. If it is, change it to `Reading`.
            // `Acquire` ordering ensures that we see the producer's write.
            // `Relaxed` for failure is fine as we will just retry or sleep.
            if self.shared.state.compare_exchange(
                State::Full as u8,
                State::Reading as u8,
                Ordering::Acquire,
                Ordering::Relaxed,
            ).is_ok() {
                // Wake the producer, in case it was waiting to know the buffer is free.
                self.shared.producer_waker.wake();

                Poll::Ready(unsafe { self.new_payload(false) })
            } else {
                // The slot is not full. Register our waker to be woken up later.
                self.shared.consumer_waker.register(cx.waker());
                Poll::Pending
            }
        }).await
    }

    async fn acquire_write(&'b self) -> Payload<'b, Self> {
        poll_fn(|cx| {
            // Atomically check if the state is `Empty`. If it is, change it to `Writing`.
            if self.shared.state.compare_exchange(
                State::Empty as u8,
                State::Writing as u8,
                Ordering::Acquire,
                Ordering::Relaxed,
            ).is_ok() {
                // Wake the consumer, in case it was waiting to know it's being written to.
                // This is often not necessary but can be useful in some protocols.
                self.shared.consumer_waker.wake();
                
                Poll::Ready(unsafe{ self.new_payload(true) })
            } else {
                // The slot is not empty. Register our waker to be woken up later.
                self.shared.producer_waker.register(cx.waker());
                Poll::Pending
            }
        }).await
    }

    fn release(&self, buf: &'b mut [u8], metadata: Metadata, is_write: bool) {
        // This is called when the payload is dropped.
        // We need to restore the buffer and metadata.
        unsafe {
            *self.buffer.get() = Some(buf);
            *self.payload_metadata.get() = Some(metadata);
        }

        if is_write {
            // Change the state to Full, allowing consumers to read from it.
            self.shared.state.store(State::Full as u8, Ordering::Release);
            self.shared.consumer_waker.wake();
        }
        else {
            self.shared.state.store(State::Empty as u8, Ordering::Release);
            self.shared.producer_waker.wake();    
        }
    }
}
