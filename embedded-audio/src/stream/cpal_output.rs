use std::sync::Arc;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::SizedSample;
use async_ringbuf::traits::{AsyncProducer, Consumer, Observer, Producer, Split};
use async_ringbuf::{AsyncHeapRb, AsyncHeapProd, AsyncHeapCons};

use embedded_audio_driver::element::{Element, ProcessResult, Eof, Fine};
use embedded_audio_driver::info::Info;
use embedded_audio_driver::port::{Dmy, InPlacePort, InPort, OutPort, PortRequirements};
use embedded_audio_driver::stream::{Stream, StreamState};
use embedded_audio_driver::Error;
use embedded_audio_driver::payload::Position;
use embedded_audio_driver::databus::{Consumer as DatabusConsumer, Producer as DatabusProducer, Transformer};

use crate::utils::FromBytes;
use crate::Channel;

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
pub struct CpalOutputStream<T: SizedSample + FromBytes<SIZE> + Send + Sync + 'static, const SIZE: usize> {
    cpal_device: cpal::Device,
    cpal_config: cpal::StreamConfig,
    stream: Option<cpal::Stream>,
    rb_producer: AsyncHeapProd<T>,
    rb_consumer: Option<AsyncHeapCons<T>>,
    flush_channel: Arc<Channel<bool, 1>>,
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
        let ring_buffer = AsyncHeapRb::<T>::new(rb_capacity_samples);
        let (mut producer, consumer) = ring_buffer.split();

        

        // Pre-fill the buffer with silence to satisfy the initial latency requirement
        let min_samples_to_fill = config.get_rb_min_capacity_bytes(&info) / std::mem::size_of::<T>();
        for _ in 0..min_samples_to_fill {
            producer
                .try_push(T::EQUILIBRIUM)
                .unwrap_or_else(|_| panic!("Initial buffer fill should not fail"));
        }

        Ok(CpalOutputStream {
            cpal_device,
            cpal_config,
            stream: None,
            rb_producer: producer,
            rb_consumer: Some(consumer),
            flush_channel: Arc::new(Channel::new()),
            info,
            state: StreamState::Uninitialized, // Stream starts in the Uninitialized state.
            _phantom: core::marker::PhantomData,
        })
    }
}

impl<T: SizedSample + FromBytes<SIZE> + Send + Sync + 'static, const SIZE: usize> Element
    for CpalOutputStream<T, SIZE>
{
    type Error = Error;

    fn get_out_info(&self) -> Option<Info> {
        None
    }

    fn get_in_info(&self) -> Option<Info> {
        Some(self.info)
    }

    fn get_port_requirements(&self) -> PortRequirements {
        PortRequirements::sink(1)
    }

    fn available(&self) -> u32 {
        (self.rb_producer.vacant_len() * std::mem::size_of::<T>()) as u32
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.flush_channel.send(true).await;
        Ok(())
    }

    async fn initialize<R, W>(
            &mut self,
            _in_port: &mut InPort<'_, R, Dmy>,
            _out_port: &mut OutPort<'_, W, Dmy>,
            _upstream_info: Option<Info>,
        ) -> Result<PortRequirements, Self::Error>
        where
            R: embedded_io::Read + embedded_io::Seek,
            W: embedded_io::Write + embedded_io::Seek {
        
        if self.state != StreamState::Uninitialized {
             return Err(Error::InvalidState);
        }
        let mut consumer = self.rb_consumer.take().expect("Consumer is only taken once during init");

        let flush_receiver = Arc::clone(&self.flush_channel);

        let err_fn = |err| eprintln!("an error occurred on the output audio stream: {}", err);
        let output_data_fn = move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let mut flushed_this_block = false;
            let mut input_fell_behind = false;

            for sample in data {
                 if flushed_this_block {
                    *sample = T::EQUILIBRIUM;
                    continue;
                }
                if flush_receiver.try_receive().is_ok() {
                    consumer.clear();
                    flushed_this_block = true;
                    *sample = T::EQUILIBRIUM;
                    continue;
                }
                *sample = match consumer.try_pop() {
                    Some(s) => s,
                    None => {
                        input_fell_behind = true;
                        T::EQUILIBRIUM
                    }
                };
            }
            if input_fell_behind {
                eprintln!("input stream fell behind: try increasing latency or buffer size");
            }
        };

        let stream = self.cpal_device.build_output_stream(&self.cpal_config, output_data_fn, err_fn, None)
            .map_err(|_| Error::DeviceError)?;
        
        self.stream = Some(stream);
        self.state = StreamState::Initialized;

        Ok(self.get_port_requirements())
    }

    async fn process<'b, R, W, C, P, TF>(
        &mut self,
        in_port: &mut InPort<'b, R, C>,
        _out_port: &mut OutPort<'b, W, P>,
        _inplace_port: &mut InPlacePort<'b, TF>,
    ) -> ProcessResult<Self::Error>
    where
        R: embedded_io::Read + embedded_io::Seek,
        W: embedded_io::Write + embedded_io::Seek,
        C: DatabusConsumer<'b>,
        P: DatabusProducer<'b>,
        TF: Transformer<'b>,
    {
        if self.state != StreamState::Running {
            return Err(Error::InvalidState); // Not running, so can't process
        }

        if let InPort::Consumer(databus) = in_port {
            let payload = databus.acquire_read().await;
            let sample_size = std::mem::size_of::<T>();

            let samples = payload
                .chunks_exact(sample_size)
                .map(|chunk| T::from_le_bytes(chunk.try_into().unwrap()));
            
            for sample in samples {
                if self.rb_producer.push(sample).await.is_err() {
                    self.state = StreamState::Stopped;
                    return Err(Error::DeviceError); // Audio thread closed
                }
            }
            
            match payload.metadata.position {
                Position::Last | Position::Single => {
                    self.state = StreamState::Stopped;
                    Ok(Eof)
                }
                _ => Ok(Fine),
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
        if let Some(stream) = self.stream.as_ref() {
            stream.play().map_err(|_| Error::DeviceError)?;
            self.state = StreamState::Running;
            Ok(())
        } else {
            Err(Error::NotInitialized)
        }
    }

    fn stop(&mut self) -> Result<(), Self::Error> {
        if let Some(stream) = self.stream.as_ref() {
            stream.pause().map_err(|_| Error::DeviceError)?;
            self.state = StreamState::Stopped;
            Ok(())
        } else {
            Err(Error::NotInitialized)
        }
    }

    fn pause(&mut self) -> Result<(), Self::Error> {
        if let Some(stream) = self.stream.as_ref() {
            stream.pause().map_err(|_| Error::DeviceError)?;
            self.state = StreamState::Paused;
            Ok(())
        } else {
            Err(Error::NotInitialized)
        }
    }

    fn resume(&mut self) -> Result<(), Self::Error> {
        if self.state == StreamState::Paused {
            if let Some(stream) = self.stream.as_ref() {
                stream.play().map_err(|_| Error::DeviceError)?;
                self.state = StreamState::Running;
            } else {
                 return Err(Error::NotInitialized);
            }
        }
        Ok(())
    }

    fn get_state(&self) -> StreamState {
        self.state
    }
}
