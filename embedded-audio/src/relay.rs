use core::fmt::Debug;
use embedded_audio_driver::element::{Element, ReaderElement, WriterElement};
use embedded_audio_driver::info::Info;

#[derive(Debug)] 
pub struct Relay<const N: usize> {
    total_samples: u64,
    samples_processed: u64,
    info: Info,
}

impl<const N: usize> Relay<N> {
    pub fn new(info: Info, total_ms: u32) -> Self {
        let samples_per_second = info.sample_rate * info.channels as u32;
        let total_samples = (samples_per_second as u64 * total_ms as u64) / 1000;
        
        Self {
            total_samples,
            samples_processed: 0,
            info,
        }
    }

    pub fn get_processed_samples(&self) -> u64 {
        self.samples_processed
    }
    
    pub fn get_info(&self) -> Info {
        self.info
    }
}

impl<const N: usize> Element for Relay<N> {
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
        if let (Some(reader), Some(writer)) = (reader, writer) {
            let read_len = self.info.down_to_alignment(
                reader.available()
                .min(writer.available()
                .min((self.total_samples - self.samples_processed) as u32 * self.info.get_alignment_bytes() as u32)
            )) as usize;
            
            let mut buf = [0u8; N];
            let actual_len = reader.read(&mut buf[..read_len]).unwrap();

            writer.write(&buf[..actual_len]).unwrap();

            assert!(actual_len % self.info.get_alignment_bytes() as usize == 0);
            self.samples_processed += actual_len as u64 / self.info.get_alignment_bytes() as u64;
            
        } else {
            panic!()
        }
        Ok(())
    }
}