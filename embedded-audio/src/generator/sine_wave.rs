use core::f32::consts::PI;
use libm::sinf;

use embedded_audio_driver::databus::{Consumer, Producer, Transformer};
use embedded_audio_driver::element::{BaseElement, ProcessResult, Eof, Fine};
use embedded_audio_driver::info::Info;
use embedded_audio_driver::payload::Position;
use embedded_audio_driver::port::{InPlacePort, InPort, OutPort, PortRequirements};
use embedded_audio_driver::Error;

/// A generator that produces a sine wave.
/// It implements the `Element` trait to be used within an audio processing pipeline.
pub struct SineWaveGenerator {
    info: Info,
    frequency: f32,
    amplitude: f32,
    current_sample: u64,
    is_first_chunk: bool,
}

impl SineWaveGenerator {
    /// Creates a new sine wave generator with the specified parameters.
    ///
    /// # Parameters
    /// * `info` - The audio format information (sample rate, channels, bits per sample).
    /// * `frequency` - The frequency of the sine wave in Hz.
    /// * `amplitude` - The amplitude of the wave, from 0.0 to 1.0.
    pub fn new(info: Info, frequency: f32, amplitude: f32) -> Self {
        if !info.vaild() {
            panic!("Invalid Info for SineWaveGenerator");
        }

        if frequency <= 0.0 || amplitude < 0.0 || amplitude > 1.0 {
            panic!("Invalid frequency or amplitude for SineWaveGenerator");
        }

        Self {
            info,
            frequency,
            amplitude,
            current_sample: 0,
            is_first_chunk: true,
        }
    }

    pub fn set_info(&mut self, info: Info) {
        if !info.vaild() {
            panic!("Invalid Info for SineWaveGenerator");
        }
        self.info = info;
    }

    pub fn set_duration_ms(&mut self, duration_ms: u32) {
        self.info.set_duration_ms(duration_ms);
    }

    pub fn set_duration_s(&mut self, duration_s: f32) {
        self.info.set_duration_s(duration_s);
    }

    pub fn set_num_frames(&mut self, num_frames: u64) {
        self.info.set_num_frames(num_frames);
    }

    /// Generates a single sample value based on the current position.
    fn generate_sample(&self, sample_idx: u64) -> f32 {
        let t = sample_idx as f32 / self.info.sample_rate as f32;
        self.amplitude * sinf(2.0 * PI * self.frequency * t)
    }

    /// Calculates the minimum required payload size for efficient processing.
    fn calculate_min_payload_size(&self) -> u16 {
        (self.info.bits_per_sample as u16 / 8) * self.info.channels as u16
    }
}

impl BaseElement for SineWaveGenerator {
    type Error = Error;
    type Info = Info;

    fn get_in_info(&self) -> Option<Info> {
        None
    }

    /// Returns the audio format information of the generated sine wave.
    fn get_out_info(&self) -> Option<Info> {
        Some(self.info)
    }

    fn get_port_requirements(&self) -> PortRequirements {
        PortRequirements::source(self.calculate_min_payload_size())
    }

    /// The generated stream is virtually infinite.
    fn available(&self) -> u32 {
        u32::MAX
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.current_sample = 0;
        self.is_first_chunk = true;
        Ok(())
    }

    async fn reset(&mut self) -> Result<(), Self::Error> {
        self.flush().await
    }

    /// The main processing function to generate sine wave data.
    async fn process<'a, C, P, T>(
        &mut self,
        _in_port: &mut InPort<'a, C>,
        out_port: &mut OutPort<'a, P>,
        _inplace_port: &mut InPlacePort<'a, T>,
    ) -> ProcessResult<Self::Error>
    where
        C: Consumer<'a>,
        P: Producer<'a>,
        T: Transformer<'a>,
    {
        // This element only supports producing data into a payload.
        if let OutPort::Producer(producer) = out_port {
            let mut payload = producer.acquire_write().await;

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

                if let Some(total) = self.info.num_frames {
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
        port::{InPlacePort, InPort},
    };

    #[tokio::test]
    async fn test_sine_wave_generator_process() {
        // Test case: Verify that the SineWaveGenerator correctly fills a payload
        // with audio data and sets the appropriate metadata.

        // 1. Setup the generator and ports
        let info = Info::new(44100, 2, 16, None);
        let mut generator = SineWaveGenerator::new(info, 440.0, 0.5);
        let mut buffer = vec![0u8; 1024]; // A buffer for the output payload
        let slot = Slot::new(Some(&mut buffer), false);

        let mut in_port = InPort::new_none();
        let mut out_port = slot.out_port();
        let mut inplace_port = InPlacePort::new_none();

        // 2. First process call
        assert_eq!(generator.current_sample, 0, "Initial sample count should be 0");
        assert!(generator.is_first_chunk, "Should be the first chunk initially");

        generator
            .process(&mut in_port, &mut out_port, &mut inplace_port)
            .await
            .expect("First process call should succeed");

        // 3. Verify state after first process
        // The payload is dropped, returning the buffer and metadata to the slot.
        // We acquire the slot for reading to inspect the metadata.
        let read_payload = slot.acquire_read().await;
        let metadata = read_payload.metadata;

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
        drop(read_payload);

        // 4. Second process call
        generator
            .process(&mut in_port, &mut out_port, &mut inplace_port)
            .await
            .expect("Second process call should succeed");
        
        // 5. Verify state after second process
        let read_payload_2 = slot.acquire_read().await;
        let metadata_after_second_read = read_payload_2.metadata;

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

        let info = Info::new(48000, 1, 24, None);
        let info2 = info.clone();

        let generator = SineWaveGenerator::new(info, 1000.0, 1.0);

        assert!(generator.get_port_requirements().out_payload.is_some());
        assert!(generator.get_in_info().is_none());
        assert_eq!(generator.get_out_info(), Some(info2));

        // Output checks
        let out_info = generator.get_out_info().unwrap();
        assert_eq!(out_info.sample_rate, 48000);
        assert_eq!(out_info.channels, 1);
        assert_eq!(out_info.bits_per_sample, 24);
        assert!(out_info.num_frames.is_none(), "Num frames should be None for an infinite stream");

        // Port requirement check
        let min_payload_size = (24 / 8) * 1;
        assert_eq!(
            generator.get_port_requirements().out_payload,
            Some(min_payload_size)
        );
        
        // Available frames check
        assert_eq!(generator.available(), u32::MAX, "Available frames should be max for an infinite stream");
    }
}
