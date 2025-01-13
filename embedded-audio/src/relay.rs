use core::fmt::Debug;

use embedded_audio_driver::element::Element;
use embedded_audio_driver::info::Info;
use embedded_io::{Read, Write};

#[derive(Debug)]
pub enum Error<RE: Debug, WE: Debug> {
    Reader(RE),
    Writer(WE),
    None,
}

pub struct Relay<R, W, RE, WE, const BUF_SIZE: usize>
where
    R: Read + Element<Error=RE> + embedded_io::ErrorType<Error=RE>,
    W: Write + Element<Error=WE> + embedded_io::ErrorType<Error=WE>,
    RE: Debug,
    WE: Debug,
{
    reader: R,
    writer: W,
    total_samples: u64,
    samples_processed: u64,
    info: Info,
    buffer: [u8; BUF_SIZE],
}

impl<R, W, RE, WE, const BUF_SIZE: usize> Relay<R, W, RE, WE, BUF_SIZE>
where
    R: Read + Element<Error=RE> + embedded_io::ErrorType<Error=RE>,
    W: Write + Element<Error=WE> + embedded_io::ErrorType<Error=WE>,
    RE: Debug,
    WE: Debug,
{
    pub fn new(reader: R, writer: W, total_ms: u32) -> Result<Self, &'static str> {
        let reader_info = reader.get_out_info().ok_or("Reader info not available")?;
        let writer_info = writer.get_in_info().ok_or("Writer info not available")?;
        
        if reader_info != writer_info {
            return Err("Audio format mismatch between reader and writer");
        }
        
        // let bytes_per_sample = (reader_info.bits_per_sample as u32 + 7) / 8;
        let samples_per_second = reader_info.sample_rate * reader_info.channels as u32;
        let total_samples = (samples_per_second as u64 * total_ms as u64) / 1000;
        
        Ok(Self {
            reader,
            writer,
            total_samples,
            samples_processed: 0,
            info: reader_info,
            buffer: [0; BUF_SIZE],
        })
    }
    
    pub fn process(&mut self) -> Result<(), Error<RE, WE>> {
        while self.samples_processed < self.total_samples {

            let bytes_per_sample = ((self.info.bits_per_sample + 7) / 8) as usize;
            let samples_per_buffer = BUF_SIZE / bytes_per_sample;
            let remaining_samples = self.total_samples - self.samples_processed;
            let samples_to_read = samples_per_buffer.min(remaining_samples as usize);
            let bytes_to_read = samples_to_read * bytes_per_sample;
            
            let mut bytes_read = 0;
            while bytes_read < bytes_to_read {
                match self.reader.read(&mut self.buffer[bytes_read..bytes_to_read]) {
                    Ok(0) => break, // EOF
                    Ok(n) => bytes_read += n,
                    Err(e) => return Err(Error::Reader(e)),
                }
            }
            
            if bytes_read == 0 {
                break;
            }
            
            let mut bytes_written = 0;
            while bytes_written < bytes_read {
                match self.writer.write(&self.buffer[bytes_written..bytes_read]) {
                    Ok(n) => bytes_written += n,
                    Err(e) => return Err(Error::Writer(e)),
                }
            }
            
            self.samples_processed += (bytes_read / bytes_per_sample) as u64;
        }
        
        Ok(())
    }
    
    pub fn get_processed_samples(&self) -> u64 {
        self.samples_processed
    }
    
    pub fn get_info(&self) -> Info {
        self.info
    }
}

impl<R, W, RE, WE, const BUF_SIZE: usize> Element for Relay<R, W, RE, WE, BUF_SIZE>
where
    R: Read + Element<Error=RE> + embedded_io::ErrorType<Error=RE>,
    W: Write + Element<Error=WE> + embedded_io::ErrorType<Error=WE>,
    RE: Debug,
    WE: Debug,
{
    type Error = core::convert::Infallible;

    fn get_in_info(&self) -> Option<Info> {
        Some(self.info)
    }
    
    fn get_out_info(&self) -> Option<Info> {
        Some(self.info)
    }
}

