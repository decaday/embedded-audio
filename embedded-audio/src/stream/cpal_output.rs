use std::sync::Arc;

use async_ringbuf::traits::{AsyncProducer, Consumer, Observer, Producer, Split};
use async_ringbuf::{AsyncHeapRb, AsyncHeapProd, AsyncHeapCons};
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::SizedSample;

use embedded_audio_driver::databus::{Consumer as DatabusConsumer, Producer as DatabusProducer, Transformer as DatabusTransformer};
use embedded_audio_driver::element::{BaseElement, ProcessResult, Eof, Fine};
use embedded_audio_driver::info::Info;
use embedded_audio_driver::payload::Position;
use embedded_audio_driver::port::{InPlacePort, InPort, OutPort, PayloadSize, PortRequirements};
use embedded_audio_driver::stream::{BaseStream, StreamState};
use embedded_audio_driver::Error;
use crate::utils::FromBytes;
use crate::Channel;

#[derive(Debug)]
pub struct Config {
    /// Ring buffer capacity in bytes. If None, a default capacity is calculated based on latency.
    pub rb_capacity: Option<usize>,
    /// The desired latency in milliseconds. This is used to calculate the minimum buffer size.
    pub latency_ms: usize,
    /// Number of frames to process in each call to `process`.
    pub frames_per_process: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rb_capacity: None,
            latency_ms: 50,
            frames_per_process: 64,
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
    fn get_rb_capacity_bytes(
        &self,
        info: &Info,
    ) -> Result<usize, CapacityTooSmallError> {
        let min_cap_bytes = self.get_rb_min_capacity_bytes(info);

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
        Ok(final_cap_bytes)
    }
}

/// An output stream that sends audio data to a CPAL device.
/// It acts as a sink Element in the audio pipeline.
pub struct CpalOutputStream<T: SizedSample + FromBytes<SIZE> + Send + Sync + 'static, const SIZE: usize> {
    cpal_device: cpal::Device,
    cpal_config: cpal::StreamConfig,
    stream: Option<cpal::Stream>,
    rb_producer: Option<AsyncHeapProd<T>>,
    rb_consumer: Option<AsyncHeapCons<T>>,
    flush_channel: Arc<Channel<bool, 1>>,
    info: Option<Info>,
    state: StreamState,
    config: Config,
    _phantom: core::marker::PhantomData<T>,
}

impl<T: SizedSample + FromBytes<SIZE> + Send + Sync + 'static, const SIZE: usize>
    CpalOutputStream<T, SIZE>
{
    pub fn new(
        config: Config,
        cpal_device: cpal::Device,
        cpal_config: cpal::StreamConfig,
    ) -> Self {
        CpalOutputStream {
            cpal_device,
            cpal_config,
            stream: None,
            rb_producer: None,
            rb_consumer: None,
            flush_channel: Arc::new(Channel::new()),
            info: None,
            state: StreamState::Uninitialized,
            config,
            _phantom: core::marker::PhantomData,
        }
    }
}

impl<T: SizedSample + FromBytes<SIZE> + Send + Sync + 'static, const SIZE: usize> BaseElement
    for CpalOutputStream<T, SIZE>
{
    type Error = Error;
    type Info = Info;

    fn get_out_info(&self) -> Option<Info> {
        None // This is a sink element.
    }

    fn get_in_info(&self) -> Option<Info> {
        self.info
    }

    fn available(&self) -> u32 {
        if let Some(producer) = &self.rb_producer {
            (producer.vacant_len() * std::mem::size_of::<T>()) as u32
        } else {
            0
        }
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.flush_channel.send(true).await;
        Ok(())
    }

    async fn initialize(
        &mut self,
        upstream_info: Option<Self::Info>,
    ) -> Result<PortRequirements, Self::Error> {
        if self.state != StreamState::Uninitialized {
            return Err(Error::InvalidState);
        }

        let info = upstream_info.ok_or(Error::InvalidParameter)?;
        self.info = Some(info);

        // --- Ring Buffer Initialization ---
        let rb_capacity_bytes = self.config.get_rb_capacity_bytes(&info).map_err(|_| Error::InvalidParameter)?;
        let rb_capacity_samples = rb_capacity_bytes / std::mem::size_of::<T>();
        let ring_buffer = AsyncHeapRb::<T>::new(rb_capacity_samples);
        let (mut producer, consumer) = ring_buffer.split();
        
        // Pre-fill the buffer with silence to satisfy the initial latency requirement
        let min_samples_to_fill = self.config.get_rb_min_capacity_bytes(&info) / std::mem::size_of::<T>();
        for _ in 0..min_samples_to_fill {
            producer
                .try_push(T::EQUILIBRIUM)
                .map_err(|_| Error::BufferFull)?; // Should not fail on a new buffer
        }
        self.rb_producer = Some(producer);
        self.rb_consumer = Some(consumer);


        // --- CPAL Stream Initialization ---
        let mut consumer = self.rb_consumer.take().expect("Consumer is only taken once during init");
        let flush_receiver = Arc::clone(&self.flush_channel);
        let err_fn = |err| eprintln!("[cpal_output] stream error: {}", err);

        let output_data_fn = move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let mut flushed_this_block = false;
            let mut input_fell_behind = false;

            for sample in data.iter_mut() {
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
                // Use a non-blocking logger or a more robust mechanism in real applications
                eprintln!("[cpal_output] buffer underrun: input stream fell behind");
            }
        };

        let stream = self
            .cpal_device
            .build_output_stream(&self.cpal_config, output_data_fn, err_fn, None)
            .map_err(|_| Error::DeviceError)?;
        
        self.stream = Some(stream);
        self.state = StreamState::Initialized;

        Ok(PortRequirements::sink(PayloadSize { 
            min: SIZE as u16, 
            preferred: SIZE as u16 * self.config.frames_per_process as u16,
        }))
    }

    async fn process<'a, C, P, TF>(
        &mut self,
        in_port: &mut InPort<'a, C>,
        _out_port: &mut OutPort<'a, P>,
        _inplace_port: &mut InPlacePort<'a, TF>,
    ) -> ProcessResult<Self::Error>
    where
        C: DatabusConsumer<'a>,
        P: DatabusProducer<'a>,
        TF: DatabusTransformer<'a>,
    {
        if self.state != StreamState::Running {
            return Err(Error::InvalidState);
        }

        let producer = self.rb_producer.as_mut().ok_or(Error::NotInitialized)?;

        if let InPort::Consumer(databus) = in_port {
            let payload = databus.acquire_read().await;
            
            let samples = payload
                .chunks_exact(SIZE)
                .map(|chunk| T::from_le_bytes(chunk.try_into().unwrap()));
            
            for sample in samples {
                if producer.push(sample).await.is_err() {
                    // This error means the audio thread (consumer) has been dropped,
                    // which is a critical failure.
                    self.state = StreamState::Stopped;
                    return Err(Error::DeviceError);
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

impl<T: SizedSample + FromBytes<SIZE> + Send + Sync + 'static, const SIZE: usize> BaseStream
    for CpalOutputStream<T, SIZE>
{
    fn start(&mut self) -> Result<(), Self::Error> {
        if self.state != StreamState::Initialized && self.state != StreamState::Stopped {
            return Err(Error::InvalidState);
        }
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
        if self.state != StreamState::Running {
            return Err(Error::InvalidState);
        }
        if let Some(stream) = self.stream.as_ref() {
            stream.pause().map_err(|_| Error::DeviceError)?;
            self.state = StreamState::Paused;
            Ok(())
        } else {
            Err(Error::NotInitialized)
        }
    }

    fn resume(&mut self) -> Result<(), Self::Error> {
        if self.state != StreamState::Paused {
            return Err(Error::InvalidState);
        }
        if let Some(stream) = self.stream.as_ref() {
            stream.play().map_err(|_| Error::DeviceError)?;
            self.state = StreamState::Running;
            Ok(())
        } else {
            Err(Error::NotInitialized)
        }
    }

    fn get_state(&self) -> StreamState {
        self.state
    }
}
