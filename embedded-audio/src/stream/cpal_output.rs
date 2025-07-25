// cpal_stream.rs

use std::sync::Arc;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::SizedSample;
use ringbuf::storage::Heap;
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::wrap::caching::Caching;
use ringbuf::{HeapRb, SharedRb};

use embedded_audio_driver::databus::Databus;
use embedded_audio_driver::element::Element;
use embedded_audio_driver::info::Info;
use embedded_audio_driver::port::{InPort, OutPort, PortRequirement};
use embedded_audio_driver::stream::{Stream, StreamState};
use embedded_audio_driver::Error;
use embedded_io::{Read, Seek, Write};

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
            * info.channels as usize
            * (info.bits_per_sample / 8) as usize
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
pub struct CpalOutputStream<T: SizedSample + FromBytes<SIZE> + Send + Sync + 'static, const SIZE: usize> {
    stream: cpal::Stream,
    rb_producer: Caching<Arc<SharedRb<Heap<T>>>, true, false>,
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
        let ring_buffer = HeapRb::new(rb_capacity_samples);
        let (mut producer, mut consumer) = ring_buffer.split();

        // Pre-fill the buffer with silence to satisfy the initial latency requirement.
        let min_samples_to_fill = config.get_rb_min_capacity_bytes(&info) / std::mem::size_of::<T>();
        for _ in 0..min_samples_to_fill {
            producer
                .try_push(T::EQUILIBRIUM)
                .unwrap_or_else(|_| panic!("Initial buffer fill should not fail"));
        }

        let err_fn = move |err| eprintln!("an error occurred on the output audio stream: {}", err);

        // This closure is called by CPAL to get more audio samples.
        let output_data_fn = move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let mut input_fell_behind = false;
            for sample in data {
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
    fn get_out_port_requriement(&self) -> PortRequirement {
        PortRequirement::None
    }

    /// This stream requires a payload as input.
    fn get_in_port_requriement(&self) -> PortRequirement {
        PortRequirement::Payload(0) // Any payload size is acceptable.
    }

    /// Returns the number of bytes that can be written to the internal buffer.
    fn available(&self) -> u32 {
        (self.rb_producer.vacant_len() * std::mem::size_of::<T>()) as u32
    }

    /// Processes an input payload, copying its data into the ring buffer for playback.
    async fn process<'a, R, W, DI, DO>(
        &mut self,
        in_port: &mut InPort<'a, R, DI>,
        _out_port: &mut OutPort<'a, W, DO>,
    ) -> Result<(), Self::Error>
    where
        R: Read + Seek,
        W: Write + Seek,
        DI: Databus<'a>,
        DO: Databus<'a>,
    {
        if let InPort::Payload(databus) = in_port {
            let payload = databus.acquire_read().await;
            let sample_size = std::mem::size_of::<T>();

            // Ensure we don't try to write more than the valid data in the payload.
            let bytes_to_read = self.available().min(payload.metadata.valid_length as u32) as usize;

            if bytes_to_read > 0 {
                let samples_written = payload.data[..bytes_to_read]
                    .chunks_exact(sample_size)
                    .map(|chunk| T::from_le_bytes(chunk.try_into().unwrap()))
                    .try_for_each(|sample| self.rb_producer.try_push(sample));

                if samples_written.is_err() {
                    // This could happen if the buffer fills up between the `available()` check
                    // and the push, which is unlikely but possible in concurrent scenarios.
                    return Err(Error::BufferFull);
                }
            }
            Ok(())
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
