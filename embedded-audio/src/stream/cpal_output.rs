use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::SizedSample;
use async_ringbuf::traits::{AsyncProducer, Consumer, Observer, Producer, Split};
use async_ringbuf::{AsyncHeapRb, AsyncHeapProd};

use embedded_audio_driver::element::{Element, ProcessResult, Eof, Fine};
use embedded_audio_driver::info::Info;
use embedded_audio_driver::port::{InPort, OutPort, PortRequirement, InPlacePort};
use embedded_audio_driver::stream::{Stream, StreamState};
use embedded_audio_driver::Error;
use embedded_audio_driver::payload::{Position, ReadPayload};
use embedded_audio_driver::databus::{Consumer as DatabusConsumer, Producer as DatabusProducer, Transformer};


use crate::utils::FromBytes;

#[derive(Debug)]
pub struct Config {
    /// Ring buffer capacity in bytes. If None, a default capacity is calculated based on latency.
    pub rb_capacity: Option<usize>,
    /// The desired latency in milliseconds. This is used to calculate the minimum buffer size.
    pub latency_ms: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rb_capacity: None,
            latency_ms: 50,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CapacityTooSmallError;

impl core::fmt::Display for CapacityTooSmallError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Provided ring buffer capacity is too small for the requested latency")
    }
}

impl std::error::Error for CapacityTooSmallError {}

impl Config {
    /// Calculates the minimum required capacity for the ring buffer based on latency.
    fn get_rb_min_capacity_bytes(&self, info: &Info) -> usize {
        self.latency_ms
            * info.sample_rate as usize
            / 1000
            * info.get_alignment_bytes() as usize
    }

    /// Determines the final ring buffer capacity, ensuring it's sufficient.
    fn get_rb_capacity_samples<T: SizedSample>(
        &self,
        info: &Info,
    ) -> Result<usize, CapacityTooSmallError> {
        let min_cap_bytes = self.get_rb_min_capacity_bytes(info);
        let sample_size = std::mem::size_of::<T>();

        let final_cap_bytes = match self.rb_capacity {
            Some(cap) => {
                // Ensure provided capacity is at least 10% larger than the minimum required.
                if cap < (min_cap_bytes as f64 * 1.1) as usize {
                    return Err(CapacityTooSmallError);
                }
                cap
            }
            // If no capacity is provided, default to twice the minimum required size.
            None => min_cap_bytes * 2,
        };
        Ok(final_cap_bytes / sample_size)
    }
}

/// An output stream that sends audio data to a CPAL device.
/// It acts as a sink Element in the audio pipeline.
// Update the struct to use the async producer.
pub struct CpalOutputStream<T: SizedSample + FromBytes<SIZE> + Send + Sync + 'static, const SIZE: usize> {
    stream: cpal::Stream,
    rb_producer: AsyncHeapProd<T>, // Use the async producer.
    info: Info,
    state: StreamState,
    _phantom: core::marker::PhantomData<T>,
}

impl<T: SizedSample + FromBytes<SIZE> + Send + Sync + 'static, const SIZE: usize>
    CpalOutputStream<T, SIZE>
{
    pub fn new(
        config: Config,
        cpal_device: cpal::Device,
        cpal_config: cpal::StreamConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let info = Info {
            sample_rate: cpal_config.sample_rate.0,
            bits_per_sample: (T::FORMAT.sample_size() * 8) as u8,
            channels: cpal_config.channels as u8,
            num_frames: None,
        };

        let rb_capacity_samples = config.get_rb_capacity_samples::<T>(&info)?;
        // Create an AsyncHeapRb for asynchronous operations.
        let ring_buffer = AsyncHeapRb::new(rb_capacity_samples);
        let (mut producer, mut consumer) = ring_buffer.split();

        // Pre-fill the buffer with silence to satisfy the initial latency requirement.
        // try_push is still available on the async producer for non-blocking pushes.
        let min_samples_to_fill = config.get_rb_min_capacity_bytes(&info) / std::mem::size_of::<T>();
        for _ in 0..min_samples_to_fill {
            producer
                .try_push(T::EQUILIBRIUM)
                .unwrap_or_else(|_| panic!("Initial buffer fill should not fail"));
        }

        let err_fn = move |err| eprintln!("an error occurred on the output audio stream: {}", err);

        // This closure is called by CPAL to get more audio samples.
        // It runs on a separate, high-priority audio thread and MUST NOT block or be async.
        let output_data_fn = move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let mut input_fell_behind = false;
            for sample in data {
                // Use non-blocking try_pop. If the buffer is empty, we fill with silence.
                // This prevents audio glitches if the processing pipeline can't keep up.
                *sample = match consumer.try_pop() {
                    Some(s) => s,
                    None => {
                        // This happens if the process() method isn't called fast enough.
                        input_fell_behind = true;
                        T::EQUILIBRIUM
                    }
                };
            }
            if input_fell_behind {
                eprintln!("input stream fell behind: try increasing latency or buffer size");
            }
        };

        let stream = cpal_device.build_output_stream(&cpal_config, output_data_fn, err_fn, None)?;

        Ok(CpalOutputStream {
            stream,
            rb_producer: producer,
            info,
            state: StreamState::Initialized, // Stream starts in the Initialized state.
            _phantom: core::marker::PhantomData,
        })
    }
}

impl<T: SizedSample + FromBytes<SIZE> + Send + Sync + 'static, const SIZE: usize> Element
    for CpalOutputStream<T, SIZE>
{
    type Error = Error;

    /// This is a sink element, so it has no output.
    fn get_out_info(&self) -> Option<Info> {
        None
    }

    /// Returns the audio info this stream expects for its input.
    fn get_in_info(&self) -> Option<Info> {
        Some(self.info)
    }

    /// This is a sink, so it has no output port requirement.
    fn get_out_port_requirement(&self) -> PortRequirement {
        PortRequirement::None
    }

    /// This stream requires a payload as input.
    fn get_in_port_requirement(&self) -> PortRequirement {
        PortRequirement::Payload { min_size: 0 } // Any payload size is acceptable.
    }

    /// Returns the number of bytes that can be written to the internal buffer.
    fn available(&self) -> u32 {
        // vacant_len() is still available on the async producer.
        (self.rb_producer.vacant_len() * std::mem::size_of::<T>()) as u32
    }

    /// Processes an input payload, pushing its data into the ring buffer for playback.
    /// This async method will now wait (yield) if the ring buffer is full.
    async fn process<'a, R, W, C, P, TF>(
        &mut self,
        in_port: &mut InPort<'a, R, C>,
        _out_port: &mut OutPort<'a, W, P>,
        _inplace_port: &mut InPlacePort<'a, TF>,
    ) -> ProcessResult<Self::Error>
    where
        R: embedded_io::Read + embedded_io::Seek,
        W: embedded_io::Write + embedded_io::Seek,
        C: DatabusConsumer<'a>,
        P: DatabusProducer<'a>,
        TF: Transformer<'a>,
    {
        if self.state != StreamState::Running {
            return Err(Error::NotInitialized);
        }

        if let InPort::Consumer(databus) = in_port {
            let payload: ReadPayload<'a, C> = databus.acquire_read().await;
            let sample_size = std::mem::size_of::<T>();

            // Create an iterator over the samples in the valid part of the payload.
            let samples = payload
                .chunks_exact(sample_size)
                .map(|chunk| T::from_le_bytes(chunk.try_into().unwrap()));
            
            // Iterate over samples and push them to the async ring buffer.
            for sample in samples {
                // .await here will pause the execution if the ring buffer is full,
                // and resume when there is space available. This provides back-pressure.
                if self.rb_producer.push(sample).await.is_err() {
                    // This error occurs if the consumer (audio thread) is dropped,
                    // which means the stream has been closed.
                    self.state = StreamState::Stopped;
                    return Err(Error::DeviceError); 
                }
            }
            
            match payload.metadata.position {
                Position::Last
                | Position::Single => {
                    self.state = StreamState::Stopped;
                    Ok(Eof)
                }
                _ => {
                    Ok(Fine)
                }
            }
        } else {
            Err(Error::Unsupported)
        }
    }
}

impl<T: SizedSample + FromBytes<SIZE> + Send + Sync + 'static, const SIZE: usize> Stream
    for CpalOutputStream<T, SIZE>
{
    fn start(&mut self) -> Result<(), Self::Error> {
        self.stream.play().map_err(|_| Error::DeviceError)?;
        self.state = StreamState::Running;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Self::Error> {
        // CPAL doesn't have a "stop" distinct from "pause". We'll treat them the same
        // but manage the state accordingly.
        self.stream.pause().map_err(|_| Error::DeviceError)?;
        self.state = StreamState::Stopped;
        Ok(())
    }

    fn pause(&mut self) -> Result<(), Self::Error> {
        self.stream.pause().map_err(|_| Error::DeviceError)?;
        self.state = StreamState::Paused;
        Ok(())
    }

    fn resume(&mut self) -> Result<(), Self::Error> {
        if self.state == StreamState::Paused {
            self.stream.play().map_err(|_| Error::DeviceError)?;
            self.state = StreamState::Running;
        }
        Ok(())
    }

    fn get_state(&self) -> StreamState {
        self.state
    }
}
