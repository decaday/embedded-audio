use embedded_audio_driver::element::WriterElement;
use embedded_io::{ErrorType, Seek, SeekFrom, Write};
use embedded_audio_driver::info::Info;
use embedded_audio_driver::encoder::{self, Encoder, EncoderState};

// use crate::{impl_element_for_encoder, impl_write_for_encoder};

pub struct WavEncoder<'a, W> {
    writer: &'a mut W,
    info: Info,
    encoded_samples: u64,
}

impl<'a, W: Write + Seek> WavEncoder<'a, W> {
    /// Creates a new WavEncoder from a writer.
    ///
    /// # Arguments
    /// * `writer` - A writer implementing the embedded-io `Write` trait, such as a file or buffer.
    /// * `info` - Information about the audio data to be encoded.
    pub fn new(writer: &'a mut W, info: Info) -> Result<Self, encoder::Error> {
        let mut encoder = Self {
            writer,
            info,
            encoded_samples: 0,
        };
        encoder.write_header()?;
        Ok(encoder)
    }

    fn write_header(&mut self) -> Result<(), encoder::Error> {
        let mut header = [0u8; 44];
        header[0..4].copy_from_slice(b"RIFF");
        header[8..12].copy_from_slice(b"WAVE");
        header[12..16].copy_from_slice(b"fmt ");
        header[16..20].copy_from_slice(&16u32.to_le_bytes()); // Subchunk1Size
        header[20..22].copy_from_slice(&1u16.to_le_bytes()); // AudioFormat
        header[22..24].copy_from_slice(&(self.info.channels as u16).to_le_bytes());
        header[24..28].copy_from_slice(&self.info.sample_rate.to_le_bytes());
        let byte_rate = self.info.sample_rate * self.info.channels as u32 * (self.info.bits_per_sample as u32 / 8);
        header[28..32].copy_from_slice(&byte_rate.to_le_bytes());
        let block_align = self.info.channels as u16 * (self.info.bits_per_sample as u16 / 8);
        header[32..34].copy_from_slice(&block_align.to_le_bytes());
        header[34..36].copy_from_slice(&(self.info.bits_per_sample as u16).to_le_bytes());
        header[36..40].copy_from_slice(b"data");
        header[40..44].copy_from_slice(&0u32.to_le_bytes()); // Placeholder for Subchunk2Size

        self.writer.write_all(&header).map_err(encoder::Error::from_io)?;
        Ok(())
    }

    fn update_data_size(&mut self) -> Result<(), encoder::Error> {
        let data_size = self.encoded_samples * self.info.channels as u64 * (self.info.bits_per_sample as u64 / 8);
        self.writer.seek(SeekFrom::Start(40)).map_err(encoder::Error::from_io)?;
        self.writer.write_all(&(data_size as u32).to_le_bytes()).map_err(encoder::Error::from_io)?;
        Ok(())
    }
}

impl<'a, W: Write + Seek> Encoder for WavEncoder<'a, W> {
    fn init(&mut self) -> Result<(), encoder::Error> {
        self.encoded_samples = 0;
        self.write_header()
    }

    fn get_state(&self) -> Result<EncoderState, encoder::Error> {
        Ok(EncoderState {
            encoded_samples: self.encoded_samples,
        })
    }

    fn stop(&mut self) -> Result<(), encoder::Error> {
        self.update_data_size()?;
        Ok(())
    }
}

impl<'a, W: Write + Seek> ErrorType for WavEncoder<'a, W> {
    type Error = encoder::Error;
}

impl<'a, W: Write + Seek> Write for WavEncoder<'a, W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        let bytes_written = self.writer.write(buf).map_err(encoder::Error::from_io)?;
        self.encoded_samples += (bytes_written as u64) / (self.info.channels as u64 * (self.info.bits_per_sample as u64 / 8));
        Ok(bytes_written)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        todo!()
    }
}

impl<'a, W: Write + Seek> WriterElement for WavEncoder<'a, W> 
{
    fn get_info(&self) -> Info {
        self.info
    }

    fn available(&self) -> u32 {
        u32::MAX
    }
}

// impl_element_for_encoder!(WavEncoder<'a, W> where W: Write + Seek);
// impl_write_for_encoder!(WavEncoder<'a, W> where W: Write + Seek);

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_io_adapters::std::FromStd;
    use std::io::Cursor;

    #[test]
    fn test_encoder_metadata() {
        let mut buffer = Vec::new();
        let mut cursor = FromStd::new(Cursor::new(&mut buffer));
        let info = Info {
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: 16,
            num_frames: None,
        };
        let encoder = WavEncoder::new(&mut cursor, info).expect("Failed to create WavEncoder");

        let info = encoder.get_info();
        assert_eq!(info.sample_rate, 44100);
        assert_eq!(info.channels, 2);
        assert_eq!(info.bits_per_sample, 16);
    }

    #[test]
    fn test_write_samples() {
        let mut buffer = Vec::new();
        let mut cursor = FromStd::new(Cursor::new(&mut buffer));
        let info = Info {
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: 16,
            num_frames: None,
        };
        let mut encoder = WavEncoder::new(&mut cursor, info).expect("Failed to create WavEncoder");

        encoder.init().expect("Failed to initialize encoder");

        let samples = vec![0u8; 1024];
        let bytes_written = encoder.write(&samples).expect("Failed to write samples");

        assert_eq!(bytes_written, 1024);
    }

    #[test]
    fn test_encoder_state() {
        let mut buffer = Vec::new();
        let mut cursor = FromStd::new(Cursor::new(&mut buffer));
        let info = Info {
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: 16,
            num_frames: None,
        };
        let mut encoder = WavEncoder::new(&mut cursor, info).expect("Failed to create WavEncoder");

        encoder.init().expect("Failed to initialize encoder");

        let samples = vec![0u8; 1024];
        encoder.write(&samples).expect("Failed to write samples");

        let state = encoder.get_state().expect("Failed to get state");
        assert_eq!(state.encoded_samples, 1024 / 4); // 1024 bytes / (2 channels * 2 bytes per sample)
    }
}