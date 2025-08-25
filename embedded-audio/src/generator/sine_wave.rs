use core::f32::consts::PI;
use embedded_io::{Read, Seek, Write};

use embedded_audio_driver::databus::Databus;
use embedded_audio_driver::element::{Element, ProcessResult, Eof, Fine};
use embedded_audio_driver::info::Info;
use embedded_audio_driver::payload::{Payload, Position};
use embedded_audio_driver::port::{InPort, OutPort, PortRequirement};
use embedded_audio_driver::Error;

/// A generator that produces a sine wave.
/// It implements the `Element` trait to be used within an audio processing pipeline.
pub struct SineWaveGenerator {
    info: Info,
    frequency: f32,
    amplitude: f32,
    current_sample: u64,
    is_first_chunk: bool,
    total_samples: Option<u64>,
}

impl SineWaveGenerator {
    /// Creates a new sine wave generator with the specified parameters.
    ///
    /// # Parameters
    /// * `sample_rate` - The number of samples per second (Hz).
    /// * `channels` - The number of audio channels (e.g., 1 for mono, 2 for stereo).
    /// * `bits_per_sample` - The number of bits per sample (e.g., 8, 16, 24).
    /// * `frequency` - The frequency of the sine wave in Hz.
    /// * `amplitude` - The amplitude of the wave, from 0.0 to 1.0.
    pub fn new(sample_rate: u32, channels: u8, bits_per_sample: u8, frequency: f32, amplitude: f32) -> Self {
        if sample_rate == 0 || channels == 0 || ![8, 16, 24, 32].contains(&bits_per_sample) || frequency <= 0.0 {
            // In a real application, this should return a Result instead of panicking.
            panic!("Invalid parameters for SineWaveGenerator");
        }

        Self {
            info: Info {
                sample_rate,
                channels,
                bits_per_sample,
                num_frames: None, // The stream is infinite
            },
            frequency,
            amplitude: amplitude.clamp(0.0, 1.0),
            current_sample: 0,
            is_first_chunk: true,
            total_samples: None, // Infinite stream
        }
    }

    pub fn set_total_samples(&mut self, total_samples: Option<u64>) {
        self.total_samples = total_samples;
    }

    pub fn set_total_secs(&mut self, total_secs: Option<f32>) {
        self.total_samples = total_secs.map(|secs| (secs * self.info.sample_rate as f32) as u64);
    }

    pub fn set_total_ms(&mut self, total_ms: Option<u32>) {
        self.total_samples = total_ms.map(|ms| (ms as u64 * self.info.sample_rate as u64) / 1000);
    }

    /// Generates a single sample value based on the current position.
    fn generate_sample(&self, sample_idx: u64) -> f32 {
        let t = sample_idx as f32 / self.info.sample_rate as f32;
        self.amplitude * (2.0 * PI * self.frequency * t).sin()
    }

    /// Calculates the minimum required payload size for efficient processing.
    fn calculate_min_payload_size(&self) -> u32 {
        let frame_size = (self.info.bits_per_sample as u32 / 8) * self.info.channels as u32;
        // Use a reasonable buffer size that's a multiple of frame size.
        let min_frames = 256;
        frame_size * min_frames
    }
}

impl Element for SineWaveGenerator {
    type Error = Error;

    /// SineWaveGenerator does not accept any input.
    fn get_in_info(&self) -> Option<Info> {
        None
    }

    /// Returns the audio format information of the generated sine wave.
    fn get_out_info(&self) -> Option<Info> {
        Some(self.info)
    }

    /// Input is not used.
    fn get_in_port_requriement(&self) -> PortRequirement {
        PortRequirement::None
    }

    /// Requires a payload for output.
    fn get_out_port_requriement(&self) -> PortRequirement {
        PortRequirement::Payload(self.calculate_min_payload_size())
    }

    /// The generated stream is virtually infinite.
    fn available(&self) -> u32 {
        u32::MAX
    }

    /// The main processing function to generate sine wave data.
    async fn process<'a, R, W, DI, DO>(
        &mut self,
        _in_port: &mut InPort<'a, R, DI>,
        out_port: &mut OutPort<'a, W, DO>,
    ) -> ProcessResult<Self::Error>
    where
        R: Read + Seek,
        W: Write + Seek,
        DI: Databus<'a>,
        DO: Databus<'a>,
    {
        // This element only supports producing data into a payload.
        if let OutPort::Payload(databus) = out_port {
            let mut payload: Payload<DO> = databus.acquire_write().await;

            let bytes_per_sample = (self.info.bits_per_sample / 8) as usize;
            let bytes_per_frame = bytes_per_sample * self.info.channels as usize;
            let max_frames = payload.len() / bytes_per_frame;
            
            if max_frames == 0 {
                return Err(Error::BufferEmpty);
            }

            let mut bytes_written = 0;
            let mut ended = false;
            for _ in 0..max_frames {
                let sample_value = self.generate_sample(self.current_sample);

                // Convert the float sample to the target integer format.
                let int_sample = match self.info.bits_per_sample {
                    8 => ((sample_value * 127.0 + 128.0) as i8) as i32,
                    16 => (sample_value * 32767.0) as i16 as i32,
                    _ => (sample_value * 8388607.0) as i32, // For 24 and 32 bits
                };

                // Write the same sample to all channels for this frame.
                for _ in 0..self.info.channels {
                    let sample_bytes = int_sample.to_le_bytes();
                    let dest_slice = &mut payload[bytes_written..bytes_written + bytes_per_sample];
                    dest_slice.copy_from_slice(&sample_bytes[..bytes_per_sample]);
                    bytes_written += bytes_per_sample;
                }

                if let Some(total) = self.total_samples {
                    if self.current_sample >= total {
                        ended = true;
                        break;
                    }
                }
                self.current_sample += 1;
            }

            payload.set_valid_length(bytes_written);

            match (ended, self.is_first_chunk) {
                (true, true) => {
                    payload.set_position(Position::Single);
                    self.is_first_chunk = false;
                    return Ok(Eof);
                }
                (true, false) => {
                    payload.set_position(Position::Last);
                    return Ok(Eof);
                }
                (false, true) => {
                    payload.set_position(Position::First);
                    self.is_first_chunk = false;
                    Ok(Fine)
                }
                (false, false) => {
                    payload.set_position(Position::Middle);
                    Ok(Fine)
                }
            }
        } else {
            Err(Error::Unsupported)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::databus::slot::Slot;
    use embedded_audio_driver::{
        payload::Position,
        port::{Dmy, InPort, OutPort},
    };

    #[tokio::test]
    async fn test_sine_wave_generator_process() {
        // Test case: Verify that the SineWaveGenerator correctly fills a payload
        // with audio data and sets the appropriate metadata.

        // 1. Setup the generator and ports
        let mut generator = SineWaveGenerator::new(44100, 2, 16, 440.0, 0.5);
        let mut buffer = vec![0u8; 1024]; // A buffer for the output payload
        let slot = Slot::new(Some(&mut buffer));

        let mut in_port: InPort<Dmy, Dmy> = InPort::None;
        let mut out_port: OutPort<Dmy, _> = OutPort::Payload(&slot);

        // 2. First process call
        assert_eq!(generator.current_sample, 0, "Initial sample count should be 0");
        assert!(generator.is_first_chunk, "Should be the first chunk initially");

        generator
            .process(&mut in_port, &mut out_port)
            .await
            .expect("First process call should succeed");

        // 3. Verify state after first process
        // The payload is dropped, returning the buffer and metadata to the slot.
        let metadata = slot
            .get_current_metadata()
            .expect("Metadata should be available after processing");

        assert_eq!(
            metadata.valid_length, 1024,
            "Payload valid length should be fully utilized"
        );
        assert_eq!(
            metadata.position,
            Position::First,
            "The first payload's position should be 'First'"
        );
        assert_eq!(
            generator.current_sample,
            256,
            "Sample count should be 1024 bytes / 4 bytes_per_frame"
        );
        assert!(!generator.is_first_chunk, "is_first_chunk should now be false");

        // Release the read payload to make the slot available for writing again.
        drop(slot.acquire_read().await);

        // 4. Second process call
        generator
            .process(&mut in_port, &mut out_port)
            .await
            .expect("Second process call should succeed");
        
        // 5. Verify state after second process
        let metadata_after_second_read = slot
            .get_current_metadata()
            .expect("Metadata should be available after second processing");

        assert_eq!(
            metadata_after_second_read.valid_length, 1024,
            "Payload valid length should be filled again"
        );
        assert_eq!(
            metadata_after_second_read.position,
            Position::Middle,
            "Subsequent payload's position should be 'Middle'"
        );
        assert_eq!(
            generator.current_sample,
            512,
            "Sample count should advance after the second call"
        );
    }

    #[test]
    fn test_sine_wave_info_and_requirements() {
        // Test case: Verify that the generator reports correct information
        // and port requirements.
        let generator = SineWaveGenerator::new(48000, 1, 24, 1000.0, 1.0);

        // Input checks
        assert_eq!(
            generator.get_in_port_requriement(),
            PortRequirement::None
        );
        assert!(generator.get_in_info().is_none());

        // Output checks
        let out_info = generator.get_out_info().unwrap();
        assert_eq!(out_info.sample_rate, 48000);
        assert_eq!(out_info.channels, 1);
        assert_eq!(out_info.bits_per_sample, 24);
        assert!(out_info.num_frames.is_none(), "Num frames should be None for an infinite stream");

        // Port requirement check
        let min_payload_size = (24 / 8) * 1 * 256;
        assert_eq!(
            generator.get_out_port_requriement(),
            PortRequirement::Payload(min_payload_size)
        );
        
        // Available frames check
        assert_eq!(generator.available(), u32::MAX, "Available frames should be max for an infinite stream");
    }
}
