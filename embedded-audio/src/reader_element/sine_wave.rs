use core::f32::consts::PI;
use std::{convert::Infallible, panic};

use embedded_audio_driver::element::ReaderElement;
use embedded_audio_driver::info::Info;

use crate::{impl_element_for_reader_element, impl_read_for_reader_element};

pub struct SineWaveGenerator {
    // Configuration parameters
    sample_rate: u32,
    channels: u8,
    bits_per_sample: u8,
    frequency: f32,
    amplitude: u8,
    
    // Internal state
    current_sample: u32,
}

impl SineWaveGenerator {
    /// Creates a new sine wave generator with the specified parameters.
    ///
    /// # Parameters
    /// * `sample_rate` - The number of samples per second (Hz)
    /// * `channels` - The number of audio channels (1 for mono, 2 for stereo)
    /// * `bits_per_sample` - The number of bits per sample (8, 16, 24, or 32)
    /// * `frequency` - The frequency of the sine wave in Hz
    /// * `amplitude` - The amplitude of the sine wave (0-255), where:
    ///                 - 0 means silence
    ///                 - 255 means maximum volume
    ///
    /// # Returns
    /// Returns a new instance of `SineWaveGenerator` configured with the specified parameters
    ///
    /// # Panics
    /// Panics if any of the parameters are invalid.
    ///
    /// # Example
    /// ```
    /// let generator = SineWaveGenerator::new(
    ///     44100,  // CD quality sample rate
    ///     2,      // Stereo
    ///     16,     // 16-bit audio
    ///     440.0,  // A4 note
    ///     128     // 50% amplitude
    /// );
    /// ```
    pub fn new(sample_rate: u32, channels: u8, bits_per_sample: u8, frequency: f32, amplitude: u8) -> Self {
        // Parameter validation
        if sample_rate == 0 {
            panic!("Sample rate must be greater than 0");
        }
        if channels == 0 {
            panic!("Channels must be greater than 0");
        }
        if ![8, 16, 24, 32].contains(&bits_per_sample) {
            panic!("Bits per sample must be one of 8, 16, 24, or 32");
        }
        if frequency <= 0.0 {
            panic!("Frequency must be greater than 0");
        }

        Self {
            sample_rate,
            channels,
            bits_per_sample,
            frequency,
            amplitude,
            current_sample: 0,
        }
    }
    
    fn generate_sample(&self, sample_idx: u32) -> f32 {
        let t = sample_idx as f32 / self.sample_rate as f32;
        // Convert amplitude from u8 to float (0-1 range)
        let amplitude_float = self.amplitude as f32 / 255.0;
        amplitude_float * (2.0 * PI * self.frequency * t).sin()
    }
}

impl ReaderElement for SineWaveGenerator {
    fn init(&mut self) -> Result<(), Infallible> {
        self.current_sample = 0;
        Ok(())
    }
    
    fn get_info(&self) -> Info {
        Info {
            sample_rate: self.sample_rate,
            channels: self.channels,
            bits_per_sample: self.bits_per_sample,
            num_frames: None,
        }
    }
    
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Infallible> {
        let bytes_per_sample = (self.bits_per_sample as usize + 7) / 8;
        let samples_to_write = buffer.len() / (bytes_per_sample * self.channels as usize);
        let mut bytes_written = 0;
        
        for _ in 0..samples_to_write {
            let sample_value = self.generate_sample(self.current_sample);
            
            // Convert float sample to integer based on bits_per_sample
            let int_sample = match self.bits_per_sample {
                8 => ((sample_value * 127.0 + 128.0) as i8) as i32,
                16 => (sample_value * 32767.0) as i16 as i32,
                24 | 32 => (sample_value * 8388607.0) as i32,
                _ => unreachable!(),
            };
            
            // Write sample to buffer for each channel
            for _ in 0..self.channels {
                match self.bits_per_sample {
                    8 => {
                        buffer[bytes_written] = int_sample as u8;
                        bytes_written += 1;
                    }
                    16 => {
                        let bytes = (int_sample as i16).to_le_bytes();
                        buffer[bytes_written..bytes_written + 2].copy_from_slice(&bytes);
                        bytes_written += 2;
                    }
                    24 => {
                        let bytes = int_sample.to_le_bytes();
                        buffer[bytes_written..bytes_written + 3].copy_from_slice(&bytes[..3]);
                        bytes_written += 3;
                    }
                    32 => {
                        let bytes = int_sample.to_le_bytes();
                        buffer[bytes_written..bytes_written + 4].copy_from_slice(&bytes);
                        bytes_written += 4;
                    }
                    _ => unreachable!(),
                }
            }
            
            self.current_sample += 1;
        }
        
        Ok(bytes_written)
    }
}

impl_element_for_reader_element!(SineWaveGenerator);
impl_read_for_reader_element!(SineWaveGenerator);