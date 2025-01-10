/// WAV decoder.
use embedded_io::{Seek, Read, SeekFrom};

use embedded_audio_driver::decoder::DecoderState;
use embedded_audio_driver::element::Element;
use embedded_audio_driver::element::Info;
use embedded_audio_driver::decoder::{self, Decoder};

pub struct WavDecoder<R> {
    reader: R,
    info: Info,
    data_start: u64,
    remaining_frames: Option<u32>,
    decoded_samples: u64,
}

impl<R: Read + Seek> WavDecoder<R> {
    /// Creates a new WavDecoder from a reader.
    ///
    /// # Arguments
    /// * `reader` - A reader implementing the embedded-io `Read` and `Seek` traits, such as a file or buffer.
    pub fn new(mut reader: R) -> Result<Self, decoder::Error> {
        let mut header = [0u8; 44];
        reader.read_exact(&mut header).map_err(decoder::Error::from_read_exact)?;

        // Validate the RIFF header
        if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
            return Err(decoder::Error::InvalidHeader);
        }

        let sample_rate = u32::from_le_bytes([header[24], header[25], header[26], header[27]]);
        let channels = header[22];
        let bits_per_sample = header[34];
        let byte_rate = u32::from_le_bytes([header[28], header[29], header[30], header[31]]);

        let num_frames = if byte_rate > 0 {
            Some((u32::from_le_bytes([header[40], header[41], header[42], header[43]]) * 8 / bits_per_sample as u32) / channels as u32)
        } else {
            None
        };

        let duration = num_frames.map(|frames| frames * 1000 / sample_rate);

        let data_start = 44; // Assume no extra chunks for simplicity

        Ok(Self {
            reader,
            info: Info {
                sample_rate,
                channels,
                bits_per_sample,
                duration,
                num_frames,
            },
            data_start,
            remaining_frames: num_frames,
            decoded_samples: 0,
        })
    }
}

impl<R: Read + Seek> Decoder for WavDecoder<R> {
    fn init(&mut self) -> Result<(), decoder::Error> {
        self.reader.seek(SeekFrom::Start(self.data_start)).map_err(decoder::Error::from)?;
        self.remaining_frames = self.info.num_frames;
        self.decoded_samples = 0;
        Ok(())
    }

    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, decoder::Error> {
        if let Some(remaining_frames) = self.remaining_frames {
            let frame_size = (self.info.bits_per_sample as usize / 8) * self.info.channels as usize;
            let max_frames = buffer.len() / frame_size;
            let frames_to_read = remaining_frames.min(max_frames as u32) as usize;

            let bytes_to_read = frames_to_read * frame_size;
            let bytes_read = self.reader.read(&mut buffer[..bytes_to_read])?;

            let frames_read = bytes_read / frame_size;
            self.remaining_frames = Some(remaining_frames - frames_read as u32);
            self.decoded_samples += frames_read as u64;

            Ok(bytes_read)
        } else {
            self.reader.read(buffer).map_err(decoder::Error::from)
        }
    }

    fn get_info(&self) -> Info {
        self.info
    }

    fn get_state(&self) -> Result<DecoderState, decoder::Error> {
        Ok(DecoderState {
            decoded_samples: self.decoded_samples,
        })
    }

    fn seek(&mut self, sample_num: u64) -> Result<(), decoder::Error> {
        let frame_size = (self.info.bits_per_sample as u64 / 8) * self.info.channels as u64;
        let byte_offset = sample_num * frame_size;
        self.reader.seek(SeekFrom::Start(self.data_start + byte_offset)).map_err(decoder::Error::from)?;
        self.remaining_frames = self.info.num_frames.map(|frames| frames - sample_num as u32);
        self.decoded_samples = sample_num;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_io_adapters::std::FromStd;
    use std::io::Cursor;

    #[test]
    fn test_decoder_metadata() {
        let wav_data =  include_bytes!("../../../res/light-rain.wav");
        let cursor = FromStd::new(Cursor::new(wav_data));
        let decoder = WavDecoder::new(cursor).expect("Failed to create WavDecoder");

        let info = decoder.get_info();
        assert_eq!(info.sample_rate, 44100);
        assert_eq!(info.channels, 2);
        assert_eq!(info.bits_per_sample, 16);
    }

    #[test]
    fn test_read_samples() {
        let wav_data = include_bytes!("../../../res/light-rain.wav");
        let cursor = FromStd::new(Cursor::new(wav_data));
        let mut decoder = WavDecoder::new(cursor).expect("Failed to create WavDecoder");

        decoder.init().expect("Failed to initialize decoder");

        let mut buffer = vec![0u8; 1024];
        let bytes_read = decoder.read(&mut buffer).expect("Failed to read samples");

        assert!(bytes_read > 0);
        assert!(bytes_read <= 1024);
    }

    #[test]
    fn test_seek() {
        let wav_data =  include_bytes!("../../../res/light-rain.wav");
        let cursor = FromStd::new(Cursor::new(wav_data));
        let mut decoder = WavDecoder::new(cursor).expect("Failed to create WavDecoder");

        decoder.init().expect("Failed to initialize decoder");

        decoder.seek(1000).expect("Failed to seek");
        let state = decoder.get_state().expect("Failed to get state");

        assert_eq!(state.decoded_samples, 1000);
    }
}