use embedded_io::{Read, Seek, SeekFrom, Write};

use embedded_audio_driver::databus::{Consumer, Producer, Transformer};
use embedded_audio_driver::element::{Element, ProcessResult, Eof, Fine};
use embedded_audio_driver::info::Info;
use embedded_audio_driver::payload::Position;
use embedded_audio_driver::port::{Dmy, InPlacePort, InPort, OutPort, PortRequirements};
use embedded_audio_driver::Error;

/// WAV encoder that implements the Element trait.
/// Uses a Consumer for input and IO for output (needs seeking for header updates).
pub struct WavEncoder {
    info: Option<Info>,
    encoded_samples: u64,
    header_written: bool,
    data_size_pos: u64,
    bytes_per_frame: u32,
    port_requirements: Option<PortRequirements>,
}

impl WavEncoder {
    /// Create a new WAV encoder.
    pub fn new() -> Self {
        Self {
            info: None,
            encoded_samples: 0,
            header_written: false,
            data_size_pos: 0,
            bytes_per_frame: 0,
            port_requirements: None,
        }
    }

    /// Set the audio format information.
    pub fn set_info(&mut self, info: Info) -> Result<(), Error> {
        if info.channels == 0 || info.bits_per_sample == 0 || info.sample_rate == 0 {
            return Err(Error::InvalidParameter);
        }

        self.bytes_per_frame = (info.bits_per_sample as u32 / 8) * info.channels as u32;
        self.info = Some(info);
        Ok(())
    }

    /// Write WAV header to output writer.
    fn write_header<W: Write + Seek>(&mut self, writer: &mut W) -> Result<(), Error> {
        // ... (rest of the function is unchanged)
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

    /// Update the data size in the header.
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

    /// Calculate minimum payload size based on audio format.
    fn calculate_min_payload_size(&self) -> u32 {
        if let Some(info) = &self.info {
            let frame_size = (info.bits_per_sample as u32 / 8) * info.channels as u32;
            // Use a reasonable buffer size that's multiple of frame size
            let min_frames = 256; // Minimum 256 frames for efficient processing
            frame_size * min_frames
        } else {
            // TODO: Default minimum size when info not available yet
            16
        }
    }

    /// Finalize the WAV file by updating header sizes.
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

    fn need_writer(&self) -> bool {
        true
    }

    fn available(&self) -> u32 {
        u32::MAX
    }

    fn get_port_requirements(&self) -> PortRequirements {
        self.port_requirements.expect("must called after initialize")
    }

    async fn initialize<'a, R, W>(
        &mut self,
        in_port: &mut InPort<'a, R, Dmy>,
        out_port: &mut OutPort<'a, W, Dmy>,
        upstream_info: Option<Info>,
    ) -> Result<PortRequirements, Self::Error>
    where
        R: Read + Seek,
        W: Write + Seek,
    {
        let _ = in_port;
        let _ = out_port;

        if let Some(info) = upstream_info {
            self.set_info(info)?;
        } else {
            return Err(Error::InvalidParameter);
        }

        let min_payload_size = self.calculate_min_payload_size();
        self.port_requirements = Some(PortRequirements::new_payload_to_writer(min_payload_size as u16));
        Ok(self.port_requirements.unwrap())
    }

    async fn reset(&mut self) -> Result<(), Self::Error> {
        self.info = None;
        self.encoded_samples = 0;
        self.port_requirements = None;
        self.header_written = false;
        self.data_size_pos = 0;
        self.bytes_per_frame = 0;

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
            (InPort::Consumer(databus), OutPort::Writer(writer)) => {
                if !self.header_written {
                    self.write_header(writer)?;
                }

                let payload = databus.acquire_read().await;

                if payload.metadata.valid_length != 0 {
                    let data_to_write = &payload[..];

                    let aligned_len = if self.bytes_per_frame > 0 {
                        (data_to_write.len() as u32 / self.bytes_per_frame) * self.bytes_per_frame
                    } else {
                        data_to_write.len() as u32
                    };

                    if aligned_len > 0 {
                        writer.write_all(&data_to_write[..aligned_len as usize]).map_err(|_| Error::DeviceError)?;
                        let frames_written = aligned_len / self.bytes_per_frame;
                        self.encoded_samples += frames_written as u64;
                    }
                }

                // If the payload is the last in a sequence, update the WAV header size fields.
                if payload.metadata.position == Position::Last || payload.metadata.position == Position::Single {
                    self.update_header_sizes(writer)?;
                    Ok(Eof)
                } else {
                    Ok(Fine)
                }
            }
            _ => Err(Error::Unsupported),
        }
    }
}


#[cfg(test)]
mod tests {
    use embedded_io::{ErrorType, Seek, SeekFrom, Write};
    
    use embedded_audio_driver::port::InPlacePort;
    use crate::databus::slot::Slot;
    use super::*;
    
    // --- Mock Writer for Testing ---
    // ... (MockWriter implementation is unchanged)
    struct MockWriter {
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
        fn get_data(&self) -> &[u8] {
            &self.data
        }
    }

    impl ErrorType for MockWriter {
        type Error = core::convert::Infallible;
    }

    impl Write for MockWriter {
        fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
            let pos = self.position as usize;
            let len = self.data.len();

            if pos >= len {
                self.data.extend_from_slice(buf);
            } else {
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
        // ... (test is unchanged)
        let mut encoder = WavEncoder::new();
        let valid_info = Info { sample_rate: 44100, channels: 2, bits_per_sample: 16, num_frames: None };
        assert!(encoder.set_info(valid_info).is_ok());
        let invalid_info = Info { sample_rate: 0, channels: 2, bits_per_sample: 16, num_frames: None };
        assert!(encoder.set_info(invalid_info).is_err());
    }

    #[tokio::test]
    async fn test_process_writes_header_and_data() {
        // Test case: Verify that `process` writes a header and data, but does not
        // finalize the header if the payload is not marked as `Last`.
        let mut encoder = WavEncoder::new();
        let mut writer = MockWriter::new();
        let info = Info { sample_rate: 44100, channels: 1, bits_per_sample: 16, num_frames: None };
        encoder.set_info(info.clone()).unwrap();

        let mut in_buffer = vec![0u8; 4];
        let slot = Slot::new(Some(&mut in_buffer), false);
        
        // A producer task fills the slot for the encoder to consume.
        {
            let mut p = slot.acquire_write().await;
            p.copy_from_slice(&[0x12, 0x34, 0x56, 0x78]);
            p.set_valid_length(4);
            p.set_position(Position::First); // Not the last packet
        } // p is dropped, slot becomes Full

        let mut in_port = slot.in_port();
        let mut out_port = OutPort::new_writer(&mut writer);
        let mut inplace_port = InPlacePort::new_none();
        encoder.initialize(&mut InPort::new_none(), &mut out_port, Some(info)).await.unwrap();

        encoder.process(&mut in_port, &mut out_port, &mut inplace_port).await.unwrap();

        assert!(encoder.header_written);
        assert_eq!(encoder.encoded_samples, 2);
        let written_data = writer.get_data();
        assert_eq!(written_data.len(), 44 + 4);
        assert_eq!(&written_data[44..48], &[0x12, 0x34, 0x56, 0x78]);
        // Header sizes should still be 0 because it wasn't the last packet.
        assert_eq!(&written_data[4..8], &[0, 0, 0, 0]);
        assert_eq!(&written_data[40..44], &[0, 0, 0, 0]);
    }

    #[tokio::test]
    async fn test_process_last_chunk_updates_header() {
        // Test case: Ensure `process` correctly finalizes the header when it
        // receives a payload marked as `Last`.
        let mut encoder = WavEncoder::new();
        let mut writer = MockWriter::new();
        encoder.set_info(Info { sample_rate: 8000, channels: 2, bits_per_sample: 16, num_frames: None }).unwrap();

        let mut in_buffer = vec![0u8; 1024];
        let slot = Slot::new(Some(&mut in_buffer), false);

        // Producer fills the slot and marks the payload as the last one.
        {
            let mut p = slot.acquire_write().await;
            p.fill(1);
            p.set_valid_length(1024);
            p.set_position(Position::Last); // The last packet
        } // p is dropped, slot becomes Full

        let mut in_port = slot.in_port();
        let mut out_port = OutPort::new_writer(&mut writer);
        let mut inplace_port = InPlacePort::new_none();
        encoder.process(&mut in_port, &mut out_port, &mut inplace_port).await.unwrap();

        assert_eq!(encoder.encoded_samples, 256);

        // Assertions: Check that the header fields were updated automatically.
        let data_after_process = writer.get_data();
        let data_size = 1024u32;
        let file_size = 36 + data_size;

        assert_eq!(&data_after_process[4..8], &file_size.to_le_bytes(), "File size was not updated correctly");
        assert_eq!(&data_after_process[40..44], &data_size.to_le_bytes(), "Data chunk size was not updated correctly");
    }
}
