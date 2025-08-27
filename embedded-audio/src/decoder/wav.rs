use embedded_io::{Read, Seek, SeekFrom, Write};

use embedded_audio_driver::databus::{Consumer, Producer, Transformer};
use embedded_audio_driver::element::{Element, ProcessResult, Eof, Fine};
use embedded_audio_driver::info::Info;
use embedded_audio_driver::payload::Position;
use embedded_audio_driver::port::{Dmy, InPlacePort, InPort, OutPort, PortRequirements};
use embedded_audio_driver::Error;

/// WAV decoder that implements the Element trait.
/// Always uses IO for input (needs seeking for header parsing) and a Producer for output.
pub struct WavDecoder {
    info: Option<Info>,
    data_start: u64,
    port_requirements: Option<PortRequirements>,
    current_position: u64,
    bytes_per_frame: u32,
    header_parsed: bool,
    is_first_chunk: bool,
}

impl WavDecoder {
    /// Create a new WAV decoder.
    pub fn new() -> Self {
        Self {
            info: None,
            data_start: 0,
            current_position: 0,
            header_parsed: false,
            bytes_per_frame: 0,
            is_first_chunk: true,
            port_requirements: None,
        }
    }

    /// Parse WAV header from input reader.
    fn parse_header<R: Read + Seek>(&mut self, reader: &mut R) -> Result<(), Error> {
        // ... (rest of the function is unchanged)
        let mut header_buf = [0u8; 44];
        reader.read_exact(&mut header_buf).map_err(|_| Error::DeviceError)?;

        // Validate RIFF header
        if &header_buf[0..4] != b"RIFF" || &header_buf[8..12] != b"WAVE" {
            return Err(Error::InvalidParameter);
        }

        // Find data chunk - handle potential extra chunks
        let mut data_chunk_pos = 36;
        loop {
            if data_chunk_pos + 8 > header_buf.len() as u64 {
                // Need to read more data to find data chunk
                reader.seek(SeekFrom::Start(data_chunk_pos)).map_err(|_| Error::DeviceError)?;
                let mut chunk_header = [0u8; 8];
                reader.read_exact(&mut chunk_header).map_err(|_| Error::DeviceError)?;
                
                if &chunk_header[0..4] == b"data" {
                    self.data_start = data_chunk_pos + 8;
                    break;
                } else {
                    // Skip this chunk
                    let chunk_size = u32::from_le_bytes([chunk_header[4], chunk_header[5], chunk_header[6], chunk_header[7]]);
                    data_chunk_pos += 8 + chunk_size as u64;
                }
            } else {
                if &header_buf[data_chunk_pos as usize..data_chunk_pos as usize + 4] == b"data" {
                    self.data_start = data_chunk_pos + 8;
                    break;
                } else {
                    // Skip this chunk
                    let chunk_size = u32::from_le_bytes([
                        header_buf[data_chunk_pos as usize + 4], 
                        header_buf[data_chunk_pos as usize + 5], 
                        header_buf[data_chunk_pos as usize + 6], 
                        header_buf[data_chunk_pos as usize + 7]
                    ]);
                    data_chunk_pos += 8 + chunk_size as u64;
                }
            }
        }

        // Extract audio parameters
        let sample_rate = u32::from_le_bytes([header_buf[24], header_buf[25], header_buf[26], header_buf[27]]);
        let channels = header_buf[22];
        let bits_per_sample = header_buf[34];
        
        if channels == 0 || bits_per_sample == 0 || sample_rate == 0 {
            return Err(Error::InvalidParameter);
        }

        self.bytes_per_frame = (bits_per_sample as u32 / 8) * channels as u32;
        
        // Calculate number of frames if data chunk size is available
        let data_chunk_size = u32::from_le_bytes([header_buf[40], header_buf[41], header_buf[42], header_buf[43]]);
        let num_frames = if self.bytes_per_frame > 0 && data_chunk_size > 0 {
            Some((data_chunk_size / self.bytes_per_frame) as u64)
        } else {
            None
        };

        self.info = Some(Info {
            sample_rate,
            channels,
            bits_per_sample,
            num_frames,
        });

        self.header_parsed = true;
        Ok(())
    }

    /// Calculate minimum payload size based on audio format.
    fn calculate_min_payload_size(&self) -> u32 {
        if let Some(info) = &self.info {
            let frame_size = (info.bits_per_sample as u32 / 8) * info.channels as u32;
            // Use a reasonable buffer size that's multiple of frame size
            let min_frames = 256; // Minimum 256 frames for efficient processing
            frame_size * min_frames
        } else {
            // TODO: Default minimum size when info not available yet
            512
        }
    }
}

impl Element for WavDecoder {
    type Error = Error;

    fn get_in_info(&self) -> Option<Info> {
        None
    }

    fn get_out_info(&self) -> Option<Info> {
        self.info
    }

    fn need_reader(&self) -> bool {
        true
    }

    fn get_port_requirements(&self) -> PortRequirements {
        self.port_requirements.expect("must called after initialize")
    }

    fn available(&self) -> u32 {
        // Return available frames if known
        if let Some(info) = &self.info {
            if let Some(num_frames) = info.num_frames {
                let processed_frames = if self.bytes_per_frame > 0 {
                    self.current_position / self.bytes_per_frame as u64
                } else {
                    0
                };
                ((num_frames as u64).saturating_sub(processed_frames)) as u32
            } else {
                u32::MAX // Unknown size, assume infinite
            }
        } else {
            u32::MAX // Not initialized yet
        }
    }

    async  fn initialize<'a, R, W>(
            &mut self,
            in_port: &mut InPort<'a, R, Dmy>,
            out_port: &mut OutPort<'a, W, Dmy>,
            _upstream_info: Option<Info>,
        ) -> Result<PortRequirements, Self::Error>
        where
            R: Read + Seek,
            W: Write + Seek {
         match (in_port, out_port) {
            (InPort::Reader(reader), _) => {
                if !self.header_parsed {
                    self.parse_header(reader)?;
                    self.port_requirements = Some(
                        PortRequirements::new_reader_to_payload(self.calculate_min_payload_size() as u16)
                    );
                    Ok(self.port_requirements.unwrap())
                } else {
                    Err(Error::InvalidState)
                }
            },
            _ => Err(Error::Unsupported),
         }
    }

    async  fn reset(&mut self) -> Result<(), Self::Error> {
        self.info = None;
        self.data_start = 0;
        self.current_position = 0;
        self.header_parsed = false;
        self.bytes_per_frame = 0;
        self.is_first_chunk = true;
        self.port_requirements = None;
        Ok(())
    }

    async fn process<'a, R, W, C, P, T>(
        &mut self,
        in_port: &mut InPort<'a, R, C>,
        out_port: &mut OutPort<'a, W, P>,
        _inplace_port: &mut InPlacePort<'a, T>,
    ) -> ProcessResult<Self::Error>
    where
        R: Read + Seek,
        W: Write + Seek,
        C: Consumer<'a>,
        P: Producer<'a>,
        T: Transformer<'a>,
    {
        match (in_port, out_port) {
            (InPort::Reader(reader), OutPort::Producer(producer)) => {
                if !self.header_parsed {
                    return Err(Error::InvalidState);
                }

                let read_pos = self.data_start + self.current_position;
                reader.seek(SeekFrom::Start(read_pos)).map_err(|_| Error::DeviceError)?;

                let mut payload = producer.acquire_write().await;
                let max_read = payload.len();
                
                let aligned_read = if self.bytes_per_frame > 0 {
                    (max_read as u32 / self.bytes_per_frame) * self.bytes_per_frame
                } else {
                    max_read as u32
                };

                if aligned_read == 0 {
                    panic!("Payload buffer too small for even one frame");
                }

                let bytes_read = reader.read(&mut payload[..aligned_read as usize]).map_err(|_| Error::DeviceError)?;

                // Set the exact number of bytes read into the payload.
                payload.set_valid_length(bytes_read);
                self.current_position += bytes_read as u64;

                // Determine if this is the first, middle, or last payload
                let mut is_last = bytes_read < aligned_read as usize;
                if !is_last {
                    if let Some(num_frames) = self.info.as_ref().and_then(|i| i.num_frames) {
                        let total_data_bytes = num_frames as u64 * self.bytes_per_frame as u64;
                        if self.current_position >= total_data_bytes {
                            is_last = true;
                        }
                    }
                }
                
                match (self.is_first_chunk, is_last) {
                    (true, true) => {
                        payload.set_position(Position::Single);
                        Ok(Eof)
                    }
                    (true, false) => {
                        payload.set_position(Position::First);
                        self.is_first_chunk = false;
                        Ok(Fine)
                    }
                    (false, true) => {
                        payload.set_position(Position::Last);
                        Ok(Eof)
                    }
                    (false, false) => {
                        payload.set_position(Position::Middle);
                        Ok(Fine)
                    }
                }
            },
            _ => Err(Error::Unsupported),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use embedded_io_adapters::std::FromStd;
    use embedded_io::ErrorType;

    use crate::databus::slot::Slot; 
    use super::*;
    
    // --- Real File Data Tests ---
    // These tests use a real WAV file included at compile time to verify
    // the decoder's behavior in a realistic scenario.
    const REAL_WAV_FILE: &[u8] = include_bytes!("../../../res/light-rain.wav");

    #[test]
    fn test_metadata_parsing_from_real_file() {
        // ... (test is unchanged)
        let mut reader = FromStd::new(Cursor::new(REAL_WAV_FILE));
        let mut decoder = WavDecoder::new();
        decoder.parse_header(&mut reader).expect("Failed to parse header from real WAV file");
        assert!(decoder.header_parsed, "Decoder should confirm header is parsed");
        let info = decoder.get_out_info().expect("Output info should be available after parsing");
        assert_eq!(info.sample_rate, 44100, "Sample rate mismatch");
        assert_eq!(info.channels, 2, "Channel count mismatch");
        assert_eq!(info.bits_per_sample, 16, "Bits per sample mismatch");
    }

    #[tokio::test]
    async fn test_process_reads_data_from_real_file() {
        // Test case: Verify the main `process` function reads audio data into
        // an output payload and correctly updates its internal state and metadata.

        let mut reader = FromStd::new(Cursor::new(REAL_WAV_FILE));
        let mut decoder = WavDecoder::new();
        
        let mut buffer = vec![0u8; 1024];
        let slot = Slot::new(Some(&mut buffer), false);

        let mut in_port = InPort::new_reader(&mut reader);
        let mut out_port = slot.out_port();
        let mut inplace_port = InPlacePort::new_none();

        // First process call
        let initial_position = decoder.current_position;
        decoder.process(&mut in_port, &mut out_port, &mut inplace_port).await.unwrap();
        
        assert!(decoder.header_parsed, "Header should be parsed after the first process call");
        assert!(decoder.current_position > initial_position, "Current position should advance after first read");
        let position_after_first_read = decoder.current_position;
        
        let payload_guard = slot.acquire_read().await;
        drop(payload_guard);
            
        // Second process call
        decoder.process(&mut in_port, &mut out_port, &mut inplace_port).await.unwrap();
        assert!(decoder.current_position > position_after_first_read, "Current position should advance after second read");

        // The metadata is now part of the payload itself, but we can check the slot's internal state.
        let final_metadata = slot.get_current_metadata().expect("Metadata should be available after processing");
        
        // The decoder should have set the valid length to the number of bytes read.
        assert!(final_metadata.valid_length > 0, "Payload valid_length should be greater than 0");
        assert_eq!(final_metadata.valid_length, 1024, "Payload valid_length should be the size of the read");

        // Since we read twice, the position should be Middle.
        assert_eq!(final_metadata.position, Position::Middle, "Payload position should be Middle");
    }

    // --- Mock Data Unit Tests ---
    // ... (These tests do not use `process` and remain unchanged)
    // A mock reader for testing purposes, implementing embedded_io traits.
    // This avoids dependency on the file system for unit tests.
    struct MockReader {
        data: Vec<u8>,
        position: u64,
    }

    impl MockReader {
        fn new(data: Vec<u8>) -> Self {
            Self { data, position: 0 }
        }
    }

    impl ErrorType for MockReader {
        type Error = core::convert::Infallible;
    }

    impl Read for MockReader {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
            let bytes_to_read = (self.data.len() as u64 - self.position).min(buf.len() as u64) as usize;
            buf[..bytes_to_read]
                .copy_from_slice(&self.data[self.position as usize..self.position as usize + bytes_to_read]);
            self.position += bytes_to_read as u64;
            Ok(bytes_to_read)
        }
    }

    impl Seek for MockReader {
        fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
            let new_pos = match pos {
                SeekFrom::Start(p) => p,
                SeekFrom::End(p) => (self.data.len() as i64 + p) as u64,
                SeekFrom::Current(p) => (self.position as i64 + p) as u64,
            };
            if new_pos <= self.data.len() as u64 {
                self.position = new_pos;
                Ok(self.position)
            } else {
                self.position = self.data.len() as u64;
                Ok(self.position)
            }
        }
    }
    
    fn create_valid_wav_data() -> Vec<u8> {
        let mut data = Vec::new();
        let num_samples = 64;
        let channels = 2u16;
        let bits_per_sample = 16u16;
        let sample_rate = 44100u32;
        let block_align = channels * (bits_per_sample / 8);
        let byte_rate = sample_rate * block_align as u32;
        let data_size = num_samples * block_align as u32;
        let file_size = 36 + data_size;

        // RIFF Chunk Descriptor
        data.extend_from_slice(b"RIFF");
        data.extend_from_slice(&file_size.to_le_bytes());
        data.extend_from_slice(b"WAVE");

        // "fmt " sub-chunk
        data.extend_from_slice(b"fmt ");
        data.extend_from_slice(&16u32.to_le_bytes()); // Sub-chunk size for PCM
        data.extend_from_slice(&1u16.to_le_bytes()); // Audio format (1 for PCM)
        data.extend_from_slice(&channels.to_le_bytes());
        data.extend_from_slice(&sample_rate.to_le_bytes());
        data.extend_from_slice(&byte_rate.to_le_bytes());
        data.extend_from_slice(&block_align.to_le_bytes());
        data.extend_from_slice(&bits_per_sample.to_le_bytes());

        // "data" sub-chunk
        data.extend_from_slice(b"data");
        data.extend_from_slice(&data_size.to_le_bytes());
        data.extend_from_slice(&vec![0; data_size as usize]); // The actual sound data

        data
    }

    #[test]
    fn test_header_parsing_with_mock_data() {
        let wav_data = create_valid_wav_data();
        let mut reader = MockReader::new(wav_data);
        let mut decoder = WavDecoder::new();

        decoder.parse_header(&mut reader).expect("Parsing generated header should succeed");

        let info = decoder.get_out_info().unwrap();
        assert_eq!(info.sample_rate, 44100);
        assert_eq!(info.channels, 2);
        assert_eq!(info.bits_per_sample, 16);
        assert_eq!(info.num_frames, Some(64));
    }

    #[test]
    fn test_invalid_header_fails_parsing() {
        // Test case: Ensure the decoder returns an error when given a file with
        // an invalid RIFF header.
        let mut invalid_data = create_valid_wav_data();
        invalid_data[0..4].copy_from_slice(b"NOPE");
        let mut reader = MockReader::new(invalid_data);
        let mut decoder = WavDecoder::new();
        decoder.parse_header(&mut reader).expect_err("Parsing invalid header should fail");
        assert!(!decoder.header_parsed, "header_parsed flag should be false after a failed parse");
    }
}
