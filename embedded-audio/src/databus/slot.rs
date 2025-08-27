use core::cell::UnsafeCell;
use core::future::poll_fn;
use core::sync::atomic::{AtomicU8, Ordering};
use core::task::Poll;

use embassy_sync::waitqueue::AtomicWaker;

use embedded_audio_driver::databus::{Consumer, Producer, Transformer};
use embedded_audio_driver::payload::{Metadata, ReadPayload, TransformPayload, WritePayload};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum State {
    /// The slot is empty and ready for a producer.
    Empty,
    /// A producer is currently writing to the buffer.
    Writing,
    /// The slot is full with raw data, ready for the next stage (transformer or consumer).
    Full,
    /// A transformer is currently modifying the buffer in-place.
    Transforming,
    /// The data has been transformed (or is ready for consumption directly), now ready ONLY for a consumer.
    Transformed,
    /// A consumer is currently reading from the buffer.
    Reading,
    /// No buffer is currently managed by the slot.
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
            x if x == State::Transforming as u8 => Ok(State::Transforming),
            x if x == State::Transformed as u8 => Ok(State::Transformed),
            x if x == State::NoneBuffer as u8 => Ok(State::NoneBuffer),
            _ => Err(()),
        }
    }
}


/// The shared state for synchronization between tasks.
struct SharedState {
    state: AtomicU8,
    producer_waker: AtomicWaker,
    /// Waker for the transformer task, which waits for the `Full` state.
    transformer_waker: AtomicWaker,
    /// Waker for the final consumer task, which waits for `Transformed`.
    consumer_waker: AtomicWaker,
}

/// A single-slot channel with a configurable state machine that correctly sequences
/// an optional in-place transformation between the producer and consumer.
pub struct Slot<'b> {
    buffer: UnsafeCell<Option<&'b mut [u8]>>,
    payload_metadata: UnsafeCell<Option<Metadata>>,
    shared: SharedState,
    /// Configuration flag to determine the pipeline structure.
    /// If true, the pipeline is Producer -> Transformer -> Consumer.
    /// If false, the pipeline is Producer -> Consumer.
    has_transformer: bool,
}

unsafe impl<'b> Sync for Slot<'b> {}

impl<'b> Slot<'b> {
    /// Creates a new Slot.
    ///
    /// # Arguments
    ///
    /// * `buffer`: The memory buffer to be managed by the slot.
    /// * `has_transformer`: A boolean indicating whether an in-place transformer is part of the pipeline.
    pub fn new(buffer: Option<&'b mut [u8]>, has_transformer: bool) -> Self {
        let initial_state = if buffer.is_some() { State::Empty } else { State::NoneBuffer };
        Slot {
            buffer: UnsafeCell::new(buffer),
            payload_metadata: UnsafeCell::new(None),
            shared: SharedState {
                state: AtomicU8::new(initial_state as u8),
                producer_waker: AtomicWaker::new(),
                transformer_waker: AtomicWaker::new(),
                consumer_waker: AtomicWaker::new(),
            },
            has_transformer,
        }
    }

    pub fn get_current_metadata(&self) -> Option<Metadata> {
        unsafe { (*self.payload_metadata.get()).clone() }
    }
}

impl<'b> Producer<'b> for Slot<'b> {
    async fn acquire_write(&'b self) -> WritePayload<'b, Self> {
        poll_fn(|cx| {
            // Attempt to transition from Empty to Writing.
            if self.shared.state.compare_exchange(
                State::Empty as u8, State::Writing as u8, Ordering::Acquire, Ordering::Relaxed
            ).is_ok() {
                // On success, take the buffer and grant write access.
                let buffer = unsafe { (*self.buffer.get()).take().expect("BUG: State was Empty but buffer was None") };
                Poll::Ready(WritePayload::new(buffer, self))
            } else {
                // Otherwise, register waker and wait.
                self.shared.producer_waker.register(cx.waker());
                Poll::Pending
            }
        }).await
    }

    fn release_write(&self, buf: &'b mut [u8], metadata: Metadata) {
        unsafe {
            *self.buffer.get() = Some(buf);
            *self.payload_metadata.get() = Some(metadata);
        }

        if self.has_transformer {
            // If a transformer is configured, transition to `Full` and wake the transformer.
            self.shared.state.store(State::Full as u8, Ordering::Release);
            self.shared.transformer_waker.wake();
        } else {
            self.shared.state.store(State::Transformed as u8, Ordering::Release);
            self.shared.consumer_waker.wake();
        }
    }
}

impl<'b> Transformer<'b> for Slot<'b> {
    async fn acquire_transform(&'b self) -> TransformPayload<'b, Self> {
        // This function should logically only be polled if `has_transformer` is true.
        // A debug assertion helps catch incorrect usage during development.
        debug_assert!(self.has_transformer, "acquire_transform called on a Slot configured without a transformer");

        poll_fn(|cx| {
            // A Transformer waits for the `Full` state.
            if self.shared.state.compare_exchange(
                State::Full as u8, State::Transforming as u8, Ordering::Acquire, Ordering::Relaxed
            ).is_ok() {
                let (buffer, metadata) = unsafe {
                    let buffer_mut = (*self.buffer.get()).take().expect("BUG: State was Full but buffer was None");
                    let metadata_ref = (*self.payload_metadata.get()).unwrap_or_default();
                    (buffer_mut, metadata_ref)
                };
                Poll::Ready(TransformPayload::new(buffer, metadata, self))
            } else {
                // It uses its own dedicated waker.
                self.shared.transformer_waker.register(cx.waker());
                Poll::Pending
            }
        }).await
    }

    fn release_transform(&self, buf: &'b mut [u8], metadata: Metadata) {
        trace!("Transformer releasing transform slot");
        unsafe {
            *self.buffer.get() = Some(buf);
            *self.payload_metadata.get() = Some(metadata);
        }
        // After transforming, the state becomes `Transformed`, ready ONLY for a consumer.
        self.shared.state.store(State::Transformed as u8, Ordering::Release);
        // It wakes ONLY the final consumer.
        self.shared.consumer_waker.wake();
    }
}


impl<'b> Consumer<'b> for Slot<'b> {
    async fn acquire_read(&'b self) -> ReadPayload<'b, Self> {
        poll_fn(|cx| {
            // The consumer waits for the `Transformed` state.
            // The producer is responsible for putting the slot into this state
            // either directly (no transformer) or indirectly via the transformer.
            if self.shared.state.compare_exchange(
                State::Transformed as u8, State::Reading as u8, Ordering::Acquire, Ordering::Relaxed
            ).is_ok() {
                let (buffer, metadata) = unsafe {
                    let buffer_ref = (*self.buffer.get()).as_ref().expect("BUG: State was Transformed but buffer was None");
                    let metadata_ref = (*self.payload_metadata.get()).unwrap_or_default();
                    (buffer_ref, metadata_ref)
                };
                Poll::Ready(ReadPayload::new(buffer, metadata, self))
            } else {
                self.shared.consumer_waker.register(cx.waker());
                Poll::Pending
            }
        }).await
    }

    fn release_read(&'b self, _consumed_bytes: usize) {
        trace!("Consumer releasing read slot");
        // After reading, the slot is always returned to the `Empty` state.
        self.shared.state.store(State::Empty as u8, Ordering::Release);
        // Wake the producer to start the cycle again.
        self.shared.producer_waker.wake();
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryFrom;
    use std::sync::atomic::Ordering;

    use tokio::sync::oneshot;
    use tokio::time::{timeout, Duration};

    /// Helper function to get the current state as a `State` enum.
    /// This assumes that the `State` enum implements `TryFrom<u8>`,
    /// which is a common pattern for enums stored atomically as integers.
    fn get_current_state(slot: &'static Slot) -> State {
        State::try_from(slot.shared.state.load(Ordering::SeqCst)).unwrap()
    }

    /// Test case 1: A simple pipeline without a transformer.
    /// Verifies the flow: Producer -> Slot -> Consumer.
    /// `has_transformer` is set to `false`.
    #[tokio::test]
    async fn test_producer_consumer_flow_no_transformer() {
        let buffer = vec![0u8; 8];
        // We use box::leak to create a 'static mutable reference, just for testing purposes.
        // Refer to examples for better usage patterns.
        let static_buffer: &'static mut [u8] = Box::leak(buffer.into_boxed_slice());
        // Configure the slot for a pipeline with NO transformer.
        let slot = Slot::new(Some(static_buffer), false);
        let static_slot: &'static Slot = Box::leak(Box::new(slot));

        // Spawn a consumer that waits for data.
        let consumer_handle = tokio::spawn(async move {
            let payload = static_slot.acquire_read().await;
            assert_eq!(payload.len(), 4);
            assert_eq!(&payload[..], &[1, 2, 3, 4]);
            // Drop payload implicitly calls release_read, setting state to Empty.
        });

        // The producer acquires the slot for writing.
        let mut write_payload = static_slot.acquire_write().await;
        assert_eq!(get_current_state(static_slot), State::Writing);
        write_payload[0..4].copy_from_slice(&[1, 2, 3, 4]);
        write_payload.set_valid_length(4);
        
        // Release the write. Since has_transformer is false, this should set the
        // state to `Transformed` and wake the consumer.
        drop(write_payload);
        assert_eq!(get_current_state(static_slot), State::Transformed);

        // The test succeeds if the consumer finishes without timing out.
        timeout(Duration::from_millis(100), consumer_handle).await.expect("Consumer timed out").unwrap();
        
        // Final state should be Empty.
        assert_eq!(get_current_state(static_slot), State::Empty);
    }

    /// Test case 2: A full pipeline including a transformer.
    /// Verifies the flow: Producer -> Slot -> Transformer -> Slot -> Consumer.
    /// `has_transformer` is set to `true`.
    #[tokio::test]
    async fn test_full_pipeline_flow_with_transformer() {
        let buffer = vec![0u8; 8];
        // We use box::leak to create a 'static mutable reference, just for testing purposes.
        // Refer to examples for better usage patterns.
        let static_buffer: &'static mut [u8] = Box::leak(buffer.into_boxed_slice());
        let slot = Slot::new(Some(static_buffer), true);
        let static_slot: &'static Slot = Box::leak(Box::new(slot));

        // Create a channel to signal the main thread from the consumer.
        let (tx, rx) = oneshot::channel();

        let consumer_handle = tokio::spawn(async move {
            rx.await.expect("Failed to receive signal");

            let payload = static_slot.acquire_read().await;
            assert_eq!(payload.len(), 4);
            assert_eq!(&payload[..], &[11, 12, 13, 14]);
            
            // The payload is dropped here, releasing the lock and setting state to Empty.
        });
        
        let transformer_handle = tokio::spawn(async move {
            let mut payload = static_slot.acquire_transform().await;
            assert_eq!(&payload[..], &[1, 2, 3, 4]);
            for byte in payload.iter_mut() {
                *byte += 10;
            }
        });

        let mut write_payload = static_slot.acquire_write().await;
        write_payload[0..4].copy_from_slice(&[1, 2, 3, 4]);
        write_payload.set_valid_length(4);
        
        drop(write_payload);
        assert_eq!(get_current_state(static_slot), State::Full);

        timeout(Duration::from_millis(100), transformer_handle).await.expect("Transformer timed out").unwrap();
        
        // At this exact point, after the transformer has finished, the state MUST be Transformed.
        // The consumer is waiting and hasn't run yet.
        assert_eq!(get_current_state(static_slot), State::Transformed);

        tx.send(()).unwrap();

        // Finally, wait for the consumer task to fully complete.
        timeout(Duration::from_millis(100), consumer_handle).await.expect("Consumer task timed out").unwrap();

        // After the consumer is done, the final state must be Empty.
        assert_eq!(get_current_state(static_slot), State::Empty);
    }

    /// Test case 3: Verify that the consumer waits for the transformer when configured.
    /// If has_transformer is true, the consumer should not acquire the slot from the `Full` state.
    #[tokio::test]
    async fn test_consumer_waits_for_transformer_when_configured() {
        let buffer = vec![0u8; 8];
        // We use box::leak to create a 'static mutable reference, just for testing purposes.
        // Refer to examples for better usage patterns.
        let static_buffer: &'static mut [u8] = Box::leak(buffer.into_boxed_slice());
        // Configure the slot WITH a transformer.
        let slot = Slot::new(Some(static_buffer), true);
        let static_slot: &'static Slot = Box::leak(Box::new(slot));

        // Spawn a consumer that will try to acquire a read lock.
        let consumer_handle = tokio::spawn(async move {
            static_slot.acquire_read().await;
        });

        // Yield to give the consumer a chance to run and register its waker.
        tokio::task::yield_now().await;

        // The producer writes data and releases the slot.
        // This sets the state to `Full` and wakes the (non-existent in this test) transformer.
        let mut write_payload = static_slot.acquire_write().await;
        write_payload.set_valid_length(4);
        drop(write_payload);
        
        // The state is now `Full`.
        assert_eq!(get_current_state(static_slot), State::Full);

        // The consumer should be pending, because it is waiting for the `Transformed` state,
        // not the `Full` state.
        // We expect the attempt to await the consumer handle to time out.
        let result = timeout(Duration::from_millis(50), consumer_handle).await;
        assert!(result.is_err(), "Consumer did not wait for transformer and acquired the slot early");
    }
}
