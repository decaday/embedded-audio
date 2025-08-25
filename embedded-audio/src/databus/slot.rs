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
    /// The slot is empty and ready for a producer to write to it.
    Empty,
    /// A consumer has acquired the slot and is currently reading from the buffer.
    Reading,
    /// A producer has acquired the slot and is currently writing to the buffer.
    Writing,
    /// The slot is full of data and ready for a consumer to read from it.
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

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_audio_driver::payload::Position;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn test_slot_acq_rel_and_metadata() {
        // Test case: Verify the basic acquire-release cycle for both writing and reading,
        // and ensure that metadata is correctly stored and retrieved.

        let mut buffer = vec![0u8; 1024];
        let slot = Slot::new(Some(&mut buffer));

        // 1. Producer acquires the slot for writing
        {
            let mut write_payload = slot.acquire_write().await;
            assert_eq!(write_payload.len(), 1024, "Write payload should have the full buffer size");

            // Write some data and set metadata
            let test_data = [1, 2, 3, 4];
            write_payload[..4].copy_from_slice(&test_data);
            write_payload.set_valid_length(4);
            write_payload.set_position(Position::First);
        } // write_payload is dropped, which calls release() and sets the state to Full.

        // 2. Verify metadata is stored in the slot after writing
        let stored_metadata = slot.get_current_metadata().expect("Metadata should be available after write");
        assert_eq!(stored_metadata.valid_length, 4, "Stored valid_length is incorrect");
        assert_eq!(stored_metadata.position, Position::First, "Stored position is incorrect");

        // 3. Consumer acquires the slot for reading
        {
            let read_payload = slot.acquire_read().await;
            assert_eq!(read_payload.len(), 1024, "Read payload should have the full buffer size");
            assert_eq!(read_payload.metadata.valid_length, 4, "Read payload metadata valid_length is incorrect");
            assert_eq!(read_payload.metadata.position, Position::First, "Read payload metadata position is incorrect");

            // Verify the data
            assert_eq!(&read_payload[..4], &[1, 2, 3, 4], "Data read does not match data written");
        } // read_payload is dropped, which calls release() and sets the state back to Empty.

        // 4. Verify we can acquire for writing again, showing the slot has been reset.
        let _ = slot.acquire_write().await;
    }

    #[tokio::test]
    async fn test_concurrent_producer_consumer() {
        // Test case: Simulate a concurrent scenario where a producer writes to the slot
        // and a consumer reads from it, ensuring proper synchronization.

        // Leak a heap-allocated buffer to get a 'static mutable reference.
        let buffer: &'static mut [u8] = Box::leak(Box::new([0u8; 8]));
        
        // Leak the Slot itself to get a 'static reference to it. This satisfies the
        // trait's lifetime requirement where the borrow of the databus must have the
        // same lifetime as the buffer it contains ('static in this case).
        let slot: &'static Slot<'static> = Box::leak(Box::new(Slot::new(Some(buffer))));

        // Use a channel to signal completion from the consumer task
        let (tx, rx) = oneshot::channel();

        // Spawn a consumer task. A 'static reference is Copy, so it can be moved.
        let consumer_handle = tokio::spawn(async move {
            let payload = slot.acquire_read().await;
            // Verify the data received from the producer
            assert_eq!(&payload.data[..payload.metadata.valid_length], &[10, 20, 30]);
            assert_eq!(payload.metadata.position, Position::Single);

            // Signal that we are done
            tx.send(()).unwrap();
        });

        // Spawn a producer task
        let producer_handle = tokio::spawn(async move {
            // This task should acquire the slot first as it's initially Empty
            let mut payload = slot.acquire_write().await;
            payload.data[0..3].copy_from_slice(&[10, 20, 30]);
            payload.set_valid_length(3);
            payload.set_position(Position::Single);
        });

        // Wait for both tasks to complete
        producer_handle.await.expect("Producer task failed");
        consumer_handle.await.expect("Consumer task failed");
        
        // Wait for the consumer's completion signal
        rx.await.expect("Failed to receive completion signal from consumer");
    }
}
