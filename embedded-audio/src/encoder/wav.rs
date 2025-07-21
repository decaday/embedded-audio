use embedded_audio_driver::element::Element;
use embedded_io::{Seek, SeekFrom, Write};
use embedded_audio_driver::info::Info;
use embedded_audio_driver::port::{InPort, OutPort, PortRequirement};
use embedded_audio_driver::Error;

/// WAV encoder that implements the Element trait
/// Uses Payload for input and IO for output (needs seeking for header updates)
pub struct WavEncoder {
    info: Option<Info>,
    encoded_samples: u64,
    header_written: bool,
    data_size_pos: u64,
    bytes_per_frame: u32,
}

impl WavEncoder {
    /// Create a new WAV encoder
    pub fn new() -> Self {
        Self {
            info: None,
            encoded_samples: 0,
            header_written: false,
            data_size_pos: 0,
            bytes_per_frame: 0,
        }
    }

    /// Set the audio format information
    pub fn set_info(&mut self, info: Info) -> Result<(), Error> {
        if info.channels == 0 || info.bits_per_sample == 0 || info.sample_rate == 0 {
            return Err(Error::InvalidParameter);
        }
        
        self.bytes_per_frame = (info.bits_per_sample as u32 / 8) * info.channels as u32;
        self.info = Some(info);
        Ok(())
    }

    /// Write WAV header to output writer
    fn write_header<W: Write + Seek>(&mut self, writer: &mut W) -> Result<(), Error> {
        let info = self.info.ok_or(Error::InvalidParameter)?;
        
        let mut header = [0u8; 44];
        
        // RIFF header
        header[0..4].copy_from_slice(b"RIFF");
        // File size will be updated later
        header[4..8].copy_from_slice(&0u32.to_le_bytes()); 
        header[8..12].copy_from_slice(b"WAVE");
        
        // Format chunk
        header[12..16].copy_from_slice(b"fmt ");
        header[16..20].copy_from_slice(&16u32.to_le_bytes()); // Subchunk1Size (PCM)
        header[20..22].copy_from_slice(&1u16.to_le_bytes()); // AudioFormat (PCM)
        header[22..24].copy_from_slice(&(info.channels as u16).to_le_bytes());
        header[24..28].copy_from_slice(&info.sample_rate.to_le_bytes());
        
        let byte_rate = info.sample_rate * info.channels as u32 * (info.bits_per_sample as u32 / 8);
        header[28..32].copy_from_slice(&byte_rate.to_le_bytes());
        
        let block_align = info.channels as u16 * (info.bits_per_sample as u16 / 8);
        header[32..34].copy_from_slice(&block_align.to_le_bytes());
        header[34..36].copy_from_slice(&(info.bits_per_sample as u16).to_le_bytes());
        
        // Data chunk
        header[36..40].copy_from_slice(b"data");
        header[40..44].copy_from_slice(&0u32.to_le_bytes()); // Placeholder for data size
        
        writer.write_all(&header).map_err(|_| Error::DeviceError)?;
        
        self.header_written = true;
        self.data_size_pos = 40; // Position of data size field
        
        Ok(())
    }

    /// Update the data size in the header
    fn update_header_sizes<W: Write + Seek>(&mut self, writer: &mut W) -> Result<(), Error> {
        let data_size = self.encoded_samples * self.bytes_per_frame as u64;
        let file_size = 36 + data_size; // RIFF chunk size = file size - 8, so file size = 44 - 8 + data_size = 36 + data_size
        
        // Update file size in RIFF header
        writer.seek(SeekFrom::Start(4)).map_err(|_| Error::DeviceError)?;
        writer.write_all(&(file_size as u32).to_le_bytes()).map_err(|_| Error::DeviceError)?;
        
        // Update data chunk size
        writer.seek(SeekFrom::Start(self.data_size_pos)).map_err(|_| Error::DeviceError)?;
        writer.write_all(&(data_size as u32).to_le_bytes()).map_err(|_| Error::DeviceError)?;
        
        // Seek back to end for continued writing
        writer.seek(SeekFrom::Start(44 + data_size)).map_err(|_| Error::DeviceError)?;
        
        Ok(())
    }

    /// Process data from Payload to Writer
    async fn process_payload_to_writer<R, W: Write + Seek>(
        &mut self,
        in_port: &mut InPort<'_, R>,
        out_port: &mut OutPort<'_, W>,
    ) -> Result<(), Error>
    where
        R: embedded_io::Read + Seek,
    {
        let InPort::Payload(consumer) = in_port else {
            return Err(Error::InvalidParameter);
        };

        let OutPort::Writer(writer) = out_port else {
            return Err(Error::InvalidParameter);
        };

        // Write header if not done yet
        if !self.header_written {
            self.write_header(writer)?;
        }

        // Acquire input payload
        let payload = consumer.acquire().await;
        
        // Ensure we write complete frames only
        let data_len = payload.len();
        let aligned_len = if self.bytes_per_frame > 0 {
            (data_len as u32 / self.bytes_per_frame) * self.bytes_per_frame
        } else {
            data_len as u32
        };

        if aligned_len == 0 {
            return Err(Error::BufferEmpty);
        }

        // Write audio data
        writer.write_all(&payload[..aligned_len as usize]).map_err(|_| Error::DeviceError)?;
        
        // Update sample count
        let frames_written = aligned_len / self.bytes_per_frame;
        self.encoded_samples += frames_written as u64;
        
        Ok(())
    }

    /// Calculate minimum payload size based on audio format
    fn calculate_min_payload_size(&self) -> u32 {
        if let Some(info) = &self.info {
            let frame_size = (info.bits_per_sample as u32 / 8) * info.channels as u32;
            // Use a reasonable buffer size that's multiple of frame size
            let min_frames = 256; // Minimum 256 frames for efficient processing
            frame_size * min_frames
        } else {
            // Default minimum size when info not available yet
            2048
        }
    }

    /// Finalize the WAV file by updating header sizes
    pub fn finalize<W: Write + Seek>(&mut self, writer: &mut W) -> Result<(), Error> {
        if self.header_written {
            self.update_header_sizes(writer)?;
        }
        Ok(())
    }
}

impl Element for WavEncoder {
    type Error = Error;

    fn get_in_info(&self) -> Option<Info> {
        self.info
    }

    fn get_out_info(&self) -> Option<Info> {
        // Output is raw WAV file data
        None
    }

    fn get_in_port_requriement(&self) -> PortRequirement {
        PortRequirement::Payload(self.calculate_min_payload_size())
    }

    fn get_out_port_requriement(&self) -> PortRequirement {
        PortRequirement::IO
    }

    fn available(&self) -> u32 {
        u32::MAX
    }

    async fn process<R, W>(
        &mut self, 
        in_port: &mut InPort<'_, R>, 
        out_port: &mut OutPort<'_, W>
    ) -> Result<(), Self::Error>
    where
        R: embedded_io::Read + Seek,
        W: Write + Seek,
    {
        match out_port {
            OutPort::Writer(_) => {
                self.process_payload_to_writer(in_port, out_port).await
            },
            _ => Err(Error::Unsupported),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_audio_driver::{info::Info, port::Dmy};
    use embedded_audio_driver::slot::Slot;
    use embedded_io::{ErrorType, Seek, SeekFrom, Write};

    // --- Mock Writer for Testing ---
    // A mock writer is essential for testing the encoder. It implements the necessary
    // `Write` and `Seek` traits and stores the output in an in-memory vector.
    // This allows us to inspect the generated byte stream to verify the encoder's output.

    struct MockWriter {
        // We use a standard Vec for ease of use in the test environment.
        data: Vec<u8>,
        position: u64,
    }

    impl MockWriter {
        fn new() -> Self {
            Self {
                data: Vec::new(),
                position: 0,
            }
        }

        // Helper to get a slice of the written data for assertions.
        fn get_data(&self) -> &[u8] {
            &self.data
        }
    }

    impl ErrorType for MockWriter {
        // For mock purposes, we can use an infallible error type.
        type Error = core::convert::Infallible;
    }

    impl Write for MockWriter {
        fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
            let pos = self.position as usize;
            let len = self.data.len();

            if pos >= len {
                // If writing at or after the end, extend the vector.
                self.data.extend_from_slice(buf);
            } else {
                // If overwriting, determine how much to overwrite and how much to append.
                let bytes_to_overwrite = (len - pos).min(buf.len());
                self.data[pos..pos + bytes_to_overwrite].copy_from_slice(&buf[..bytes_to_overwrite]);
                if buf.len() > bytes_to_overwrite {
                    self.data.extend_from_slice(&buf[bytes_to_overwrite..]);
                }
            }
            self.position += buf.len() as u64;
            Ok(buf.len())
        }

        fn flush(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    impl Seek for MockWriter {
        fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
            let new_pos = match pos {
                SeekFrom::Start(p) => p as i64,
                SeekFrom::End(p) => self.data.len() as i64 + p,
                SeekFrom::Current(p) => self.position as i64 + p,
            };

            if new_pos < 0 {
                // Seeking before the start is an error in many contexts,
                // but for this mock we'll just clamp to 0.
                self.position = 0;
            } else {
                self.position = new_pos as u64;
            }
            Ok(self.position)
        }
    }

    // --- Encoder Unit Tests ---

    #[test]
    fn test_set_valid_and_invalid_info() {
        // Test case: Ensure the encoder accepts valid audio info and rejects invalid info.
        let mut encoder = WavEncoder::new();

        // Test with valid info
        let valid_info = Info {
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: 16,
            num_frames: None,
        };
        assert!(encoder.set_info(valid_info).is_ok(), "Setting valid info should succeed");
        assert_eq!(encoder.get_in_info(), Some(valid_info));
        assert_eq!(encoder.bytes_per_frame, 4, "Bytes per frame should be 4 for 16-bit stereo");

        // Test with invalid info (e.g., zero sample rate)
        let invalid_info = Info {
            sample_rate: 0, // Invalid
            channels: 2,
            bits_per_sample: 16,
            num_frames: None,
        };
        assert!(encoder.set_info(invalid_info).is_err(), "Setting invalid info should fail");
    }

    #[tokio::test]
    async fn test_process_writes_header_and_data() {
        // Test case: Verify that the first call to `process` writes a valid WAV header,
        // and subsequent calls write the audio data correctly.

        // Setup: Create the encoder, writer, and input data.
        let mut encoder = WavEncoder::new();
        let mut writer = MockWriter::new();
        let info = Info {
            sample_rate: 44100,
            channels: 1, // Mono for simplicity
            bits_per_sample: 16,
            num_frames: None,
        };
        encoder.set_info(info).unwrap();

        // Create an input payload with some mock audio data.
        let mut in_buffer = vec![0; 4]; // Two 16-bit mono samples
        let slot = Slot::new(Some(&mut in_buffer));
        let (producer, consumer) = slot.split();
        producer.acquire().await.copy_from_slice(&[0x12, 0x34, 0x56, 0x78]); 

        // The Element trait requires specific port types.
        let mut in_port = InPort::<Dmy>::Payload(&consumer);
        let mut out_port = OutPort::Writer(&mut writer);

        // Action: Process the data.
        encoder.process(&mut in_port, &mut out_port).await.unwrap();

        // Assertions
        assert!(encoder.header_written, "Header should be written after first process call");

        // Verify the number of encoded samples is tracked correctly.
        // 4 bytes / (1 channel * 2 bytes/sample) = 2 samples
        assert_eq!(encoder.encoded_samples, 2, "Encoded samples count is incorrect");

        let written_data = writer.get_data();
        assert_eq!(written_data.len(), 44 + 4, "Total written size should be header + data");

        // Verify header content
        assert_eq!(&written_data[0..4], b"RIFF", "RIFF marker is incorrect");
        assert_eq!(&written_data[8..12], b"WAVE", "WAVE marker is incorrect");
        assert_eq!(&written_data[36..40], b"data", "data marker is incorrect");

        // Verify that the audio data was written after the header.
        assert_eq!(&written_data[44..48], &[0x12, 0x34, 0x56, 0x78], "Audio data was not written correctly");
    }

    #[tokio::test]
    async fn test_finalize_updates_header_sizes() {
        // Test case: Ensure the `finalize` method correctly seeks back and updates
        // the size fields in the header after all data has been written.
        let mut encoder = WavEncoder::new();
        let mut writer = MockWriter::new();
        let info = Info {
            sample_rate: 8000,
            channels: 2,
            bits_per_sample: 16,
            num_frames: None,
        };
        encoder.set_info(info).unwrap();

        // Write 256 frames of data (256 * 4 bytes/frame = 1024 bytes)
        let mut in_buffer = vec![0u8; 1024];
        let slot = Slot::new(Some(&mut in_buffer));
        let (producer, consumer) = slot.split();
        let mut in_port = InPort::<Dmy>::Payload(&consumer);
        let mut out_port = OutPort::Writer(&mut writer);

        producer.acquire().await.fill(1); 

        encoder.process(&mut in_port, &mut out_port).await.unwrap();
        assert_eq!(encoder.encoded_samples, 256, "Should have encoded 256 samples");

        // Before finalizing, the size fields should be 0.
        let data_before_finalize = writer.get_data();
        assert_eq!(&data_before_finalize[4..8], &[0, 0, 0, 0], "File size should be 0 before finalize");
        assert_eq!(&data_before_finalize[40..44], &[0, 0, 0, 0], "Data size should be 0 before finalize");

        // Action: Finalize the encoder.
        encoder.finalize(&mut writer).expect("Finalize failed");

        // Assertions: Check that the header fields were updated.
        let data_after_finalize = writer.get_data();
        let data_size = 1024u32;
        let file_size = 36 + data_size; // As per WAV spec

        assert_eq!(
            &data_after_finalize[4..8],
            &file_size.to_le_bytes(),
            "File size in RIFF header was not updated correctly"
        );
        assert_eq!(
            &data_after_finalize[40..44],
            &data_size.to_le_bytes(),
            "Data chunk size was not updated correctly"
        );
        
        // Final check on writer position
        assert_eq!(writer.position, (44 + data_size) as u64, "Writer position should be at the end of the file");
    }
}
