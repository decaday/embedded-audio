use core::fmt::Debug;
use embedded_audio_driver::element::{Element, ReaderElement, WriterElement};
use embedded_audio_driver::info::Info;

#[derive(Debug)] 
pub struct Relay<const BUF_SIZE: usize> {
    total_samples: u64,
    samples_processed: u64,
    info: Info,
    buffer: [u8; BUF_SIZE],
}

impl<const BUF_SIZE: usize> Relay<BUF_SIZE> {
    pub fn new(info: Info, total_ms: u32) -> Self {
        let samples_per_second = info.sample_rate * info.channels as u32;
        let total_samples = (samples_per_second as u64 * total_ms as u64) / 1000;
        
        Self {
            total_samples,
            samples_processed: 0,
            info,
            buffer: [0; BUF_SIZE],
        }
    }

    pub fn get_processed_samples(&self) -> u64 {
        self.samples_processed
    }
    
    pub fn get_info(&self) -> Info {
        self.info
    }
}

impl<const BUF_SIZE: usize> Element for Relay<BUF_SIZE> {
    type Error = &'static str;

    fn get_in_info(&self) -> Option<Info> {
        Some(self.info)
    }
    
    fn get_out_info(&self) -> Option<Info> {
        Some(self.info)
    }

    fn process<R, W>(&mut self, reader: Option<&mut R>, writer: Option<&mut W>) -> Result<(), Self::Error> 
    where
        R: ReaderElement,
        W: WriterElement,
    {
        let Some(reader) = reader else {
            return Err("Reader not provided");
        };
        let Some(writer) = writer else {
            return Err("Writer not provided");
        };

        if reader.get_info() != writer.get_info() {
            return Err("Audio format mismatch between reader and writer");
        }

        while self.samples_processed < self.total_samples {
            let bytes_per_sample = ((self.info.bits_per_sample + 7) / 8) as usize;
            let samples_per_buffer = BUF_SIZE / bytes_per_sample;
            let remaining_samples = self.total_samples - self.samples_processed;
            let samples_to_read = samples_per_buffer.min(remaining_samples as usize);
            let bytes_to_read = samples_to_read * bytes_per_sample;
            
            let mut bytes_read = 0;
            while bytes_read < bytes_to_read {
                match reader.read(&mut self.buffer[bytes_read..bytes_to_read]) {
                    Ok(0) => break,
                    Ok(n) => bytes_read += n,
                    Err(_) => return Err("Read error"),
                }
            }
            
            if bytes_read == 0 {
                break;
            }
            
            let mut bytes_written = 0;
            while bytes_written < bytes_read {
                match writer.write(&self.buffer[bytes_written..bytes_read]) {
                    Ok(n) => bytes_written += n,
                    Err(_) => return Err("Write error"),
                }
            }
            
            self.samples_processed += (bytes_read / bytes_per_sample) as u64;
        }
        
        Ok(())
    }
}