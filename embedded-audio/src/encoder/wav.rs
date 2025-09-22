//! A simple WAV Encoder.

use embedded_io::{Seek, SeekFrom, Write};

use embedded_audio_driver::databus::{Consumer, Producer, Transformer};
use embedded_audio_driver::element::{BaseElement, ProcessResult, Eof, Fine};
use embedded_audio_driver::info::Info;
use embedded_audio_driver::payload::Position;
use embedded_audio_driver::port::{InPlacePort, InPort, OutPort, PayloadSize, PortRequirements};
use embedded_audio_driver::Error;

/// A WAV encoder.
///
/// This element consumes audio data from an input port and writes it into a
/// WAV-formatted stream using an internal writer that implements `Write` and `Seek`.
pub struct WavEncoder<W: Write + Seek> {
    writer: W,
    info: Option<Info>,
    encoded_frames: u64,
    header_written: bool,
    data_size_pos: u64,
    bytes_per_frame: u32,
    frames_per_process: u16,
}

impl<W: Write + Seek> WavEncoder<W> {
    /// Creates a new WAV encoder with a given writer.
    pub fn new(writer: W, frames_per_process: u16) -> Self {
        Self {
            writer,
            info: None,
            encoded_frames: 0,
            header_written: false,
            data_size_pos: 0,
            bytes_per_frame: 0,
            frames_per_process,
        }
    }

    /// Writes the WAV header to the output writer.
    fn write_header(&mut self) -> Result<(), Error>
    where
        <W as embedded_io::ErrorType>::Error: core::fmt::Debug,
    {
        let info = self.info.ok_or(Error::NotInitialized)?;
        
        let mut header = [0u8; 44];
        
        // RIFF header
        header[0..4].copy_from_slice(b"RIFF");
        header[4..8].copy_from_slice(&0u32.to_le_bytes()); // File size placeholder
        header[8..12].copy_from_slice(b"WAVE");
        
        // "fmt " chunk
        header[12..16].copy_from_slice(b"fmt ");
        header[16..20].copy_from_slice(&16u32.to_le_bytes()); // Subchunk1Size for PCM
        header[20..22].copy_from_slice(&1u16.to_le_bytes());  // AudioFormat (1 for PCM)
        header[22..24].copy_from_slice(&(info.channels as u16).to_le_bytes());
        header[24..28].copy_from_slice(&info.sample_rate.to_le_bytes());
        
        let byte_rate = info.sample_rate * info.channels as u32 * (info.bits_per_sample as u32 / 8);
        header[28..32].copy_from_slice(&byte_rate.to_le_bytes());
        
        let block_align = info.channels as u16 * (info.bits_per_sample as u16 / 8);
        header[32..34].copy_from_slice(&block_align.to_le_bytes());
        header[34..36].copy_from_slice(&(info.bits_per_sample as u16).to_le_bytes());
        
        // "data" chunk
        header[36..40].copy_from_slice(b"data");
        header[40..44].copy_from_slice(&0u32.to_le_bytes()); // Data size placeholder
        
        self.writer.write_all(&header).map_err(|_| Error::DeviceError)?;
        
        self.header_written = true;
        self.data_size_pos = 40; // Position of the data size field in the header
        
        Ok(())
    }

    /// Updates the size fields in the WAV header after all data has been written.
    fn update_header_sizes(&mut self) -> Result<(), Error>
    where
        <W as embedded_io::ErrorType>::Error: core::fmt::Debug,
    {
        let data_size = self.encoded_frames * self.bytes_per_frame as u64;
        let file_size = 36 + data_size;

        // Update file size in RIFF header
        self.writer.seek(SeekFrom::Start(4)).map_err(|_| Error::DeviceError)?;
        self.writer.write_all(&(file_size as u32).to_le_bytes()).map_err(|_| Error::DeviceError)?;

        // Update data chunk size
        self.writer.seek(SeekFrom::Start(self.data_size_pos)).map_err(|_| Error::DeviceError)?;
        self.writer.write_all(&(data_size as u32).to_le_bytes()).map_err(|_| Error::DeviceError)?;

        // Seek back to the end of the file for any subsequent operations.
        self.writer.seek(SeekFrom::Start(44 + data_size)).map_err(|_| Error::DeviceError)?;

        Ok(())
    }

    /// Finalizes the WAV file by updating header sizes.
    /// This should be called if the stream ends unexpectedly without a `Last` or `Single` payload.
    pub fn finalize(&mut self) -> Result<(), Error>
    where
        <W as embedded_io::ErrorType>::Error: core::fmt::Debug,
    {
        if self.header_written {
            self.update_header_sizes()?;
        }
        Ok(())
    }
}

impl<W: Write + Seek> BaseElement for WavEncoder<W>
where
    <W as embedded_io::ErrorType>::Error: core::fmt::Debug,
{
    type Error = Error;
    type Info = Info;

    fn get_in_info(&self) -> Option<Info> {
        self.info
    }

    fn get_out_info(&self) -> Option<Info> {
        None // This is a sink element.
    }
    
    fn available(&self) -> u32 {
        u32::MAX // Can always accept data.
    }
    
    async fn initialize(
        &mut self,
        upstream_info: Option<Self::Info>,
    ) -> Result<PortRequirements, Self::Error> {
        let info = upstream_info.ok_or(Error::InvalidParameter)?;
        if !info.vaild() {
            return Err(Error::InvalidParameter);
        }

        self.bytes_per_frame = info.get_alignment_bytes() as u32;
        self.info = Some(info);

        Ok(PortRequirements::sink( PayloadSize { 
            min: self.bytes_per_frame as u16, 
            preferred: self.bytes_per_frame as u16 * self.frames_per_process 
        }))
    }

    async fn reset(&mut self) -> Result<(), Self::Error> {
        self.info = None;
        self.encoded_frames = 0;
        self.header_written = false;
        self.data_size_pos = 0;
        self.bytes_per_frame = 0;
        // TODO: The internal writer is NOT reset. A new instance should be created for a new file.
        Ok(())
    }

    async fn process<'a, C, P, T>(
        &mut self,
        in_port: &mut InPort<'a, C>,
        _out_port: &mut OutPort<'a, P>,
        _inplace_port: &mut InPlacePort<'a, T>,
    ) -> ProcessResult<Self::Error>
    where
        C: Consumer<'a>,
        P: Producer<'a>,
        T: Transformer<'a>,
    {
        if let InPort::Consumer(databus) = in_port {
            if !self.header_written {
                self.write_header()?;
            }

            let payload = databus.acquire_read().await;
            
            if payload.is_empty() {
                 // If the last payload is empty, we still need to finalize the header.
                 if payload.metadata.position == Position::Last || payload.metadata.position == Position::Single {
                    self.update_header_sizes()?;
                    return Ok(Eof);
                }
                return Ok(Fine);
            }
            
            let data_to_write = &payload[..];

            // Ensure we only write full frames.
            let aligned_len = (data_to_write.len() as u32 / self.bytes_per_frame) * self.bytes_per_frame;

            if aligned_len > 0 {
                self.writer.write_all(&data_to_write[..aligned_len as usize]).map_err(|_| Error::DeviceError)?;
                let frames_written = aligned_len / self.bytes_per_frame;
                self.encoded_frames += frames_written as u64;
            }

            // If this is the last payload, update the header with the final sizes.
            if payload.metadata.position == Position::Last || payload.metadata.position == Position::Single {
                self.update_header_sizes()?;
                Ok(Eof)
            } else {
                Ok(Fine)
            }
        } else {
            Err(Error::Unsupported)
        }
    }
}


#[cfg(test)]
mod tests {
    use embedded_io::{ErrorType, Seek, SeekFrom, Write};
    use embedded_audio_driver::databus::{Consumer, Producer, Operation, Databus};
    use crate::databus::slot::HeapSlot;
    use super::*;
    
    // A mock writer for testing purposes.
    struct MockWriter {
        data: Vec<u8>,
        position: u64,
    }

    impl MockWriter {
        fn new() -> Self {
            Self { data: Vec::new(), position: 0 }
        }
        fn get_data(&self) -> &[u8] {
            &self.data
        }
    }
    
    // MockWriter needs ErrorType to implement Write/Seek
    impl ErrorType for MockWriter {
        type Error = core::convert::Infallible;
    }

    impl Write for MockWriter {
        fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
            let pos = self.position as usize;
            if pos >= self.data.len() {
                self.data.extend_from_slice(buf);
            } else {
                let bytes_to_overwrite = (self.data.len() - pos).min(buf.len());
                self.data[pos..pos + bytes_to_overwrite].copy_from_slice(&buf[..bytes_to_overwrite]);
                if buf.len() > bytes_to_overwrite {
                    self.data.extend_from_slice(&buf[bytes_to_overwrite..]);
                }
            }
            self.position += buf.len() as u64;
            Ok(buf.len())
        }
        fn flush(&mut self) -> Result<(), Self::Error> { Ok(()) }
    }

    impl Seek for MockWriter {
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
    
    #[tokio::test]
    async fn test_process_writes_header_and_data() {
        let writer = MockWriter::new();
        let mut encoder = WavEncoder::new(writer, 64);
        let info = Info { sample_rate: 44100, channels: 1, bits_per_sample: 16, num_frames: None };
        
        let requirements = encoder.initialize(Some(info)).await.unwrap();

        let mut slot = HeapSlot::new_heap(4);
        slot.register(Operation::Consume, requirements.in_.unwrap());
        slot.register(Operation::Produce, requirements.in_.unwrap());
        
        {
            let mut p = slot.acquire_write().await;
            p.copy_from_slice(&[0x12, 0x34, 0x56, 0x78]);
            p.set_valid_length(4);
            p.set_position(Position::First);
        }

        let mut in_port = slot.in_port();
        let mut out_port = OutPort::new_none();
        let mut in_place_port = InPlacePort::new_none();

        encoder.process(&mut in_port, &mut out_port, &mut in_place_port).await.unwrap();

        assert!(encoder.header_written);
        assert_eq!(encoder.encoded_frames, 2); // 4 bytes / (16 bits/8 * 1 channel) = 2 frames
        let written_data = encoder.writer.get_data();
        assert_eq!(written_data.len(), 44 + 4);
        assert_eq!(&written_data[44..48], &[0x12, 0x34, 0x56, 0x78]);
        // Header sizes should still be 0 because it wasn't the last packet.
        assert_eq!(&written_data[4..8], &[0, 0, 0, 0]);
        assert_eq!(&written_data[40..44], &[0, 0, 0, 0]);
    }

    #[tokio::test]
    async fn test_process_last_chunk_updates_header() {
        let writer = MockWriter::new();
        let mut encoder = WavEncoder::new(writer, 300);
        let info = Info { sample_rate: 8000, channels: 2, bits_per_sample: 16, num_frames: None };
        
        let requirements = encoder.initialize(Some(info)).await.unwrap();

        let mut slot = HeapSlot::new_heap(1024);
        slot.register(Operation::Consume, requirements.in_.unwrap());
        slot.register(Operation::Produce, requirements.in_.unwrap());

        {
            let mut p = slot.acquire_write().await;
            p.fill(1);
            p.set_valid_length(1024);
            p.set_position(Position::Last); // This IS the last packet
        }

        let mut in_port = slot.in_port();
        let mut out_port = OutPort::new_none();
        let mut in_place_port = InPlacePort::new_none();
        
        encoder.process(&mut in_port, &mut out_port, &mut in_place_port).await.unwrap();

        assert_eq!(encoder.encoded_frames, 256); // 1024 bytes / (16 bits/8 * 2 channels) = 256 frames

        let data_after_process = encoder.writer.get_data();
        let data_size = 1024u32;
        let file_size = 36 + data_size;

        assert_eq!(&data_after_process[4..8], &file_size.to_le_bytes(), "File size was not updated correctly");
        assert_eq!(&data_after_process[40..44], &data_size.to_le_bytes(), "Data chunk size was not updated correctly");
    }
}
