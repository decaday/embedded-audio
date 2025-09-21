use embedded_io::{Read, Seek, SeekFrom};

use embedded_audio_driver::databus::{Producer};
use embedded_audio_driver::element::{BaseElement, ProcessResult, Eof, Fine};
use embedded_audio_driver::info::Info;
use embedded_audio_driver::payload::Position;
use embedded_audio_driver::port::{InPlacePort, InPort, OutPort, PayloadSize, PortRequirements};
use embedded_audio_driver::Error;

/// A Simlpe WAV decoder
///
/// This element reads data from an internal reader that implements `Read` and `Seek`,
/// parses the WAV format, and produces a raw audio data stream.
pub struct WavDecoder<R: Read + Seek> {
    reader: R,
    info: Option<Info>,
    data_start: u64,
    data_end: u64,
    current_frame: u64,
    bytes_per_frame: u8,
    is_first_chunk: bool,
    frames_per_process: u16,
}

impl<R: Read + Seek> WavDecoder<R> {
    /// Creates a new WAV decoder with a given reader.
    pub fn new(reader: R,  frames_per_process: u16) -> Self {
        Self {
            reader,
            info: None,
            data_start: 0,
            data_end: 0,
            current_frame: 0,
            bytes_per_frame: 0,
            is_first_chunk: true,
            frames_per_process,
        }
    }

    /// Parses the WAV header from the internal reader.
    fn parse_header(&mut self) -> Result<(), Error>
    where
        <R as embedded_io::ErrorType>::Error: core::fmt::Debug,
    {
        let mut header_buf = [0u8; 12]; // Read only the RIFF header first
        self.reader.read_exact(&mut header_buf).map_err(|_| Error::DeviceError)?;

        if &header_buf[0..4] != b"RIFF" || &header_buf[8..12] != b"WAVE" {
            return Err(Error::InvalidParameter);
        }

        // Search for "fmt " and "data" chunks
        let mut fmt_chunk_found = false;
        let mut data_chunk_found = false;

        let mut info = Info::default();

        loop {
            let mut chunk_header = [0u8; 8];
            if self.reader.read_exact(&mut chunk_header).is_err() {
                // Reached end of file before finding both required chunks
                break;
            }
            let chunk_id = &chunk_header[0..4];
            let chunk_size = u32::from_le_bytes(chunk_header[4..8].try_into().unwrap());

            match chunk_id {
                b"fmt " => {
                    let mut fmt_buf = [0u8; 16];
                    self.reader.read_exact(&mut fmt_buf).map_err(|_| Error::DeviceError)?;
                    
                    info.channels = u16::from_le_bytes(fmt_buf[2..4].try_into().unwrap()) as u8;
                    info.sample_rate = u32::from_le_bytes(fmt_buf[4..8].try_into().unwrap());
                    info.bits_per_sample = u16::from_le_bytes(fmt_buf[14..16].try_into().unwrap()) as u8;

                    if !info.vaild() {
                        return Err(Error::InvalidParameter);
                    }
                    self.bytes_per_frame = info.get_alignment_bytes();

                    // Skip rest of fmt chunk if it's larger than 16
                    if chunk_size > 16 {
                        self.reader.seek(SeekFrom::Current((chunk_size - 16) as i64)).map_err(|_| Error::DeviceError)?;
                    }
                    fmt_chunk_found = true;
                }
                b"data" => {
                    self.data_start = self.reader.seek(SeekFrom::Current(0)).map_err(|_| Error::DeviceError)?;
                    self.data_end = self.data_start + chunk_size as u64;
                    
                    if self.bytes_per_frame > 0 {
                        info.num_frames = Some((chunk_size / self.bytes_per_frame as u32) as u64);
                    }
                    data_chunk_found = true;
                }
                _ => {
                    // Skip unknown chunks
                    self.reader.seek(SeekFrom::Current(chunk_size as i64)).map_err(|_| Error::DeviceError)?;
                }
            }

            if fmt_chunk_found && data_chunk_found {
                self.info = Some(info);
                return Ok(());
            }
        }

        Err(Error::InvalidParameter) // Required chunks not found
    }
}

impl<R: Read + Seek> BaseElement for WavDecoder<R>
where
    <R as embedded_io::ErrorType>::Error: core::fmt::Debug,
{
    type Error = Error;
    type Info = Info;

    fn get_in_info(&self) -> Option<Info> {
        None // This is a source element
    }

    fn get_out_info(&self) -> Option<Info> {
        self.info
    }

    fn available(&self) -> u32 {
        if let Some(info) = &self.info {
            if let Some(num_frames) = info.num_frames {
                return num_frames.saturating_sub(self.current_frame) as u32;
            }
        }
        u32::MAX // If num_frames is unknown, assume a large number.
    }

    async fn initialize(
        &mut self,
        _upstream_info: Option<Self::Info>,
    ) -> Result<PortRequirements, Self::Error> {
        self.parse_header()?;
        let min = self.info.unwrap().get_alignment_bytes();
        self.bytes_per_frame = min;
        Ok(PortRequirements::source(PayloadSize { 
            min: min as _,
            preferred: min as u16 * self.frames_per_process as u16,
        }))
    }

    async fn reset(&mut self) -> Result<(), Self::Error> {
        self.info = None;
        self.data_start = 0;
        self.data_end = 0;
        self.current_frame = 0;
        self.bytes_per_frame = 0;
        self.is_first_chunk = true;
        self.reader.seek(SeekFrom::Start(0)).map_err(|_| Error::DeviceError)?;
        Ok(())
    }

    async fn process<'a, C, P, T>(
        &mut self,
        _in_port: &mut InPort<'a, C>,
        out_port: &mut OutPort<'a, P>,
        _inplace_port: &mut InPlacePort<'a, T>,
    ) -> ProcessResult<Self::Error>
    where
        C: embedded_audio_driver::databus::Consumer<'a>,
        P: Producer<'a>,
        T: embedded_audio_driver::databus::Transformer<'a>,
    {
        if let OutPort::Producer(producer) = out_port {
            let current_pos_bytes = self.data_start + (self.current_frame * self.bytes_per_frame as u64);
            if current_pos_bytes >= self.data_end {
                return Ok(Eof);
            }
            
            self.reader.seek(SeekFrom::Start(current_pos_bytes)).map_err(|_| Error::DeviceError)?;

            let mut payload = producer.acquire_write().await;
            
            // Limit read to the max payload size and remaining data in the chunk.
            let max_read = (self.data_end - current_pos_bytes)
                .min(payload.len() as u64) as usize;
            let aligned_read = (max_read as u32 / self.bytes_per_frame as u32) * self.bytes_per_frame as u32;

            if aligned_read == 0 {
                panic!("Payload buffer too small for even one frame");
            }

            let bytes_read = self.reader.read(&mut payload[..aligned_read as usize]).map_err(|_| Error::DeviceError)?;
            payload.set_valid_length(bytes_read);
            
            let frames_read = bytes_read as u64 / self.bytes_per_frame as u64;
            self.current_frame += frames_read;

            let is_last = (self.data_start + (self.current_frame * self.bytes_per_frame as u64)) >= self.data_end;
            
            match (self.is_first_chunk, is_last) {
                (true, true) => payload.set_position(Position::Single),
                (true, false) => {
                    payload.set_position(Position::First);
                    self.is_first_chunk = false;
                }
                (false, true) => payload.set_position(Position::Last),
                (false, false) => payload.set_position(Position::Middle),
            }

            if is_last { Ok(Eof) } else { Ok(Fine) }

        } else {
            Err(Error::Unsupported)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_io::ErrorType;
    use embedded_io_adapters::std::FromStd;
    use std::io::Cursor;

    use crate::databus::slot::Slot;
    use embedded_audio_driver::databus::{Consumer, Operation, Producer, Databus};

    // --- Real File Data Integration Test ---
    const REAL_WAV_FILE: &[u8] = include_bytes!("../../../res/light-rain.wav");

    #[test]
    fn test_metadata_parsing_from_real_file() {
        let reader = FromStd::new(Cursor::new(REAL_WAV_FILE));
        let mut decoder = WavDecoder::new(reader, 64);
        decoder.parse_header().expect("Failed to parse header from real WAV file");
        let info = decoder.get_out_info().expect("Output info should be available after parsing");
        assert_eq!(info.sample_rate, 44100, "Sample rate mismatch");
        assert_eq!(info.channels, 2, "Channel count mismatch");
        assert_eq!(info.bits_per_sample, 16, "Bits per sample mismatch");
    }

    #[tokio::test]
    async fn test_initialize_and_process_real_file() {
        let reader = FromStd::new(Cursor::new(REAL_WAV_FILE));
        let mut decoder = WavDecoder::new(reader, 2048);

        // 1. Initialize should correctly parse header
        let requirements = decoder.initialize(None).await.expect("Initialization failed");
        assert!(requirements.out.is_some());

        let info = decoder.get_out_info().expect("Output info should be available after init");
        assert_eq!(info.sample_rate, 44100);
        assert_eq!(info.channels, 2);
        assert_eq!(info.bits_per_sample, 16);
        assert!(info.num_frames.is_some());

        // 2. Process should read data and set metadata correctly
        let mut buffer = vec![0u8; 1024];
        let mut slot = Slot::new(Some(&mut buffer));
        slot.register(Operation::Produce, requirements.out.unwrap());
        slot.register(Operation::Consume, requirements.out.unwrap());

        let mut in_port = InPort::new_none();
        let mut out_port = slot.out_port();
        let mut in_place_port = InPlacePort::new_none();

        let result = decoder.process(&mut in_port, &mut out_port, &mut in_place_port).await.unwrap();
        assert_eq!(result, Fine);

        let current_frame_after_1 = decoder.current_frame;
        assert!(current_frame_after_1 > 0);

        {
            let payload = slot.acquire_read().await;
            assert_eq!(payload.metadata.valid_length, 1024);
            assert_eq!(payload.metadata.position, Position::First);
        } // payload is dropped

        // 3. Process again should read the next chunk
        let result2 = decoder.process(&mut in_port, &mut out_port, &mut in_place_port).await.unwrap();
        assert_eq!(result2, Fine);
        assert!(decoder.current_frame > current_frame_after_1);

        {
            let payload2 = slot.acquire_read().await;
            assert_eq!(payload2.metadata.valid_length, 1024);
            assert_eq!(payload2.metadata.position, Position::Middle);
        }
    }

    // --- Mock Data Unit Tests ---

    // A mock reader for testing purposes, implementing embedded_io traits.
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
            if bytes_to_read == 0 {
                return Ok(0);
            }
            buf[..bytes_to_read]
                .copy_from_slice(&self.data[self.position as usize..self.position as usize + bytes_to_read]);
            self.position += bytes_to_read as u64;
            Ok(bytes_to_read)
        }
    }

    impl Seek for MockReader {
        fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
            let new_pos = match pos {
                SeekFrom::Start(p) => p as i64,
                SeekFrom::End(p) => self.data.len() as i64 + p,
                SeekFrom::Current(p) => self.position as i64 + p,
            };
            self.position = if new_pos < 0 { 0 } else { new_pos as u64 };
            Ok(self.position)
        }
    }
    
    // Helper to generate valid WAV file data for tests.
    fn create_valid_wav_data() -> Vec<u8> {
        let mut data = Vec::new();
        let num_frames = 64;
        let channels = 2u16;
        let bits_per_sample = 16u16;
        let sample_rate = 44100u32;
        let bytes_per_frame = (channels * (bits_per_sample / 8)) as u32;
        let data_size = num_frames * bytes_per_frame;
        let file_size = 44 - 8 + data_size;

        // RIFF Chunk Descriptor
        data.extend_from_slice(b"RIFF");
        data.extend_from_slice(&file_size.to_le_bytes());
        data.extend_from_slice(b"WAVE");

        // "fmt " sub-chunk
        data.extend_from_slice(b"fmt ");
        data.extend_from_slice(&16u32.to_le_bytes()); // Sub-chunk size for PCM
        data.extend_from_slice(&1u16.to_le_bytes());  // Audio format (1 for PCM)
        data.extend_from_slice(&channels.to_le_bytes());
        data.extend_from_slice(&sample_rate.to_le_bytes());
        let byte_rate = sample_rate * bytes_per_frame;
        data.extend_from_slice(&byte_rate.to_le_bytes());
        data.extend_from_slice(&(bytes_per_frame as u16).to_le_bytes());
        data.extend_from_slice(&bits_per_sample.to_le_bytes());

        // "data" sub-chunk
        data.extend_from_slice(b"data");
        data.extend_from_slice(&data_size.to_le_bytes());
        data.extend_from_slice(&vec![0; data_size as usize]); // The actual audio data

        data
    }

    #[tokio::test]
    async fn test_header_parsing_with_mock_data() {
        let wav_data = create_valid_wav_data();
        let reader = MockReader::new(wav_data);
        let mut decoder = WavDecoder::new(reader, 64);

        decoder.initialize(None).await.expect("Parsing generated header should succeed");

        let info = decoder.get_out_info().unwrap();
        assert_eq!(info.sample_rate, 44100);
        assert_eq!(info.channels, 2);
        assert_eq!(info.bits_per_sample, 16);
        assert_eq!(info.num_frames, Some(64));
    }

    #[tokio::test]
    async fn test_invalid_header_fails_parsing() {
        // Test case: Ensure initialize returns an error for an invalid RIFF header.
        let mut invalid_data = create_valid_wav_data();
        invalid_data[0..4].copy_from_slice(b"NOPE"); // Corrupt the header
        let reader = MockReader::new(invalid_data);
        let mut decoder = WavDecoder::new(reader, 64);

        let result = decoder.initialize(None).await;
        assert!(result.is_err(), "Parsing invalid header should fail");
        assert!(matches!(result.unwrap_err(), Error::InvalidParameter));
        assert!(decoder.get_out_info().is_none(), "Info should not be set after a failed parse");
    }
}
