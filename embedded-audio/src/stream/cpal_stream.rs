use std::sync::Arc;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::SizedSample;
use ringbuf::storage::Heap;
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::wrap::caching::Caching;
use ringbuf::{HeapRb, SharedRb};

use embedded_audio_driver::stream::Stream;
use embedded_audio_driver::element::{Element, ReaderElement, WriterElement};
use embedded_audio_driver::info::Info;
use crate::utils::FromBytes;


#[derive(Debug)]
pub struct Config {
    /// Ring buffer capacity, if None, the default capacity is used
    pub rb_capacity: Option<usize>,
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

#[derive(Debug)]
pub struct CapacityTooSmallError {}

impl core::fmt::Display for CapacityTooSmallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ring buffer capacity too small")
    }
}

impl core::error::Error for CapacityTooSmallError {}

impl Config {
    fn get_rb_min_capacity(&self, info: &Info) -> usize {
        self.latency_ms as usize *
            info.sample_rate as usize / 1000 *
            info.channels as usize * 
            info.bits_per_sample as usize / 8
    }

    fn get_rb_capacity(&self, info: &Info) -> Result<usize, CapacityTooSmallError> {
        let min_cap = self.get_rb_min_capacity(info);
        match self.rb_capacity {
            Some(cap) => {
                if cap < (min_cap as f64 * 1.1) as usize {
                    Err(CapacityTooSmallError {})
                } else {
                    Ok(cap)
                }
            },
            None => {
                Ok(min_cap as usize * 2)
            },
        }
    }
}


pub struct CpalOutputStream<T: SizedSample + FromBytes<SIZE> + Send + Sync + 'static, const SIZE: usize> {
    stream: cpal::Stream,
    rb_producer: Caching<Arc<SharedRb<Heap<T>>>, true, false>,
    info: Info,
    _phantom: core::marker::PhantomData<T>,
}

impl<T: SizedSample + FromBytes<SIZE> + Send + Sync + 'static, const SIZE: usize> CpalOutputStream<T, SIZE> {
    pub fn new(
        config: Config,
        cpal_device: cpal::Device,
        cpal_config: cpal::StreamConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let info = Info {
            sample_rate: cpal_config.sample_rate.0.try_into()?,
            bits_per_sample: (T::FORMAT.sample_size() * 8).try_into()?,
            channels: cpal_config.channels as u8,
            num_frames: None,
        };

        let rb_capacity = config.get_rb_capacity(&info)?;

        let ring_buffer = HeapRb::new(rb_capacity);
        let (mut producer, mut consumer) = ring_buffer.split();

        println!("rb min capacity: {}", config.get_rb_min_capacity(&info));
        for _ in 0..config.get_rb_min_capacity(&info) {
            producer.try_push(T::EQUILIBRIUM);
        }
        
        let err_fn = move |err| eprintln!("an error occurred on the output audio stream: {}", err);

        let output_data_fn = move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let mut input_fell_behind = false;
            for sample in data {
                *sample = match consumer.try_pop() {
                    Some(s) => s,
                    None => {
                        input_fell_behind = true;
                        T::EQUILIBRIUM
                    }
                };
            }
            if input_fell_behind {
                eprintln!("input stream fell behind: try increasing latency");
            }
        };

        let stream = cpal_device.build_output_stream(&cpal_config, output_data_fn, err_fn, None)?;

        Ok(CpalOutputStream {
            stream,
            rb_producer: producer,
            info,
            _phantom: core::marker::PhantomData,
        })
    }
}

impl<T: SizedSample + FromBytes<SIZE> + Send + Sync + 'static, const SIZE: usize> Element for CpalOutputStream<T, SIZE> {
    type Error = cpal::StreamError;

    fn process<R, W>(&mut self, reader: Option<&mut R>, _writer: Option<&mut W>) -> Result<(), Self::Error>
    where 
        R: ReaderElement,
        W: WriterElement,
    {
        let reader = reader.unwrap();

        let read_len = 
            self.info.down_to_alignment(
            reader.available()
            .min(self.rb_producer.vacant_len() as u32 * T::FORMAT.sample_size() as u32)
            .min(128)) as usize;
        
        if read_len != 0 {
            let mut buf = [0u8; 128];

            let actual_len = reader.read(&mut buf[0..read_len]).unwrap();
            if !self.info.is_aligned(actual_len) {
                panic!();
            }

            buf[0..actual_len].chunks(T::FORMAT.sample_size()).for_each(|chunk| {
                let sample = T::from_le_bytes(chunk.try_into().unwrap());
                if self.rb_producer.try_push(sample).is_err(){
                    panic!();
                }
            });
        }
        Ok(())
    }
    
    fn get_in_info(&self) -> Option<embedded_audio_driver::info::Info> {
        Some(self.info)
    }
    
    fn get_out_info(&self) -> Option<embedded_audio_driver::info::Info> {
        None
    }
}

impl<T: SizedSample + FromBytes<SIZE> + Send + Sync + 'static, const SIZE: usize> Stream for CpalOutputStream<T, SIZE> {
    fn start(&mut self) -> Result<(), embedded_audio_driver::stream::Error> {
        self.stream.play().unwrap();
        Ok(())
    }

    fn stop(&mut self) -> Result<(), embedded_audio_driver::stream::Error> {
        self.stream.pause().unwrap();
        Ok(())
    }

    fn pause(&mut self) -> Result<(), embedded_audio_driver::stream::Error> {
        self.stream.pause().unwrap();
        Ok(())
    }

    fn resume(&mut self) -> Result<(), embedded_audio_driver::stream::Error> {
        self.stream.play().unwrap();
        Ok(())
    }

    fn get_state(&self) -> embedded_audio_driver::stream::StreamState {
        todo!()
    }
}
