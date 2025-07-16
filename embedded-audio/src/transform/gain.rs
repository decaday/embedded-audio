use embedded_audio_driver::element::{Element, ReaderElement, WriterElement};
use embedded_audio_driver::info::Info;
use embedded_io::{Read, Write};

pub struct GainAmplifier<const N: usize> {
    gain: f32,
    info: Info,
}

impl<const N: usize> GainAmplifier<N> {
    pub fn new(gain: f32, info: Info) -> Self {
        Self { gain, info }
    }

    fn apply_gain_to_buffer(&self, buffer: &mut [u8]) {
        let bytes_per_sample = (self.info.bits_per_sample as usize + 7) / 8;
        
        for sample in buffer.chunks_mut(bytes_per_sample) {
            match bytes_per_sample {
                1 => {
                    let normalized = (sample[0] as f32 - 128.0) / 128.0;
                    let amplified = normalized * self.gain;
                    let clamped = amplified.clamp(-1.0, 1.0);
                    sample[0] = ((clamped * 128.0) + 128.0) as u8;
                },
                2 => {
                    let value = u16::from_le_bytes([sample[0], sample[1]]);
                    let normalized = (value as f32 - 32768.0) / 32768.0;
                    let amplified = normalized * self.gain;
                    let clamped = amplified.clamp(-1.0, 1.0);
                    let processed = ((clamped * 32768.0) + 32768.0) as u16;
                    let bytes = processed.to_le_bytes();
                    sample.copy_from_slice(&bytes);
                },
                3 => {
                    let value = ((sample[2] as u32) << 16) | ((sample[1] as u32) << 8) | (sample[0] as u32);
                    let normalized = (value as f32 - 8388608.0) / 8388608.0;
                    let amplified = normalized * self.gain;
                    let clamped = amplified.clamp(-1.0, 1.0);
                    let processed = ((clamped * 8388608.0) + 8388608.0) as u32;
                    sample[0] = (processed & 0xFF) as u8;
                    sample[1] = ((processed >> 8) & 0xFF) as u8;
                    sample[2] = ((processed >> 16) & 0xFF) as u8;
                },
                4 => {
                    let value = u32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]);
                    let normalized = (value as f64 - 2147483648.0) / 2147483648.0;
                    let amplified = (normalized * self.gain as f64) as f32;
                    let clamped = amplified.clamp(-1.0, 1.0);
                    let processed = ((clamped as f64 * 2147483648.0) + 2147483648.0) as u32;
                    let bytes = processed.to_le_bytes();
                    sample.copy_from_slice(&bytes);
                },
                _ => {}
            }
        }
    }
}

impl<const N: usize> Element for GainAmplifier<N> {
    type Error = core::convert::Infallible;

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
                reader.available().min(writer.available())) as usize;
            
            let mut buf = [0u8; N];
            let actual_len = reader.read(&mut buf[..read_len]).unwrap();
            self.apply_gain_to_buffer(&mut buf[..actual_len]);

            writer.write(&buf[..actual_len]).unwrap();
            
        } else {
            panic!()
        }
        Ok(())
    }
}

struct GainReader<'a, R: ReaderElement> {
    reader: &'a mut R,
    gain: f32,
}

struct GainWriter<'a, W: WriterElement> {
    writer: &'a mut W,
    gain: f32,
}

impl<const N: usize> TransformElement for GainAmplifier<N> {
    fn get_reader<R: ReaderElement>(&mut self, reader: &mut R) -> impl ReaderElement {
        GainReader {
            reader,
            gain: self.gain,
        }
    }

    fn get_writer<W: WriterElement>(&mut self, writer: &mut W) -> impl WriterElement {
        GainWriter {
            writer,
            gain: self.gain,
        }
    }
}

impl<'a, R: ReaderElement> Read for GainReader<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let bytes_read = self.reader.read(buf)?;
        let mut temp_buf = [0u8; 4096];
        temp_buf[..bytes_read].copy_from_slice(&buf[..bytes_read]);
        
        let bytes_per_sample = (self.reader.get_info().bits_per_sample as usize + 7) / 8;
        for chunk in temp_buf[..bytes_read].chunks_mut(bytes_per_sample) {
            match bytes_per_sample {
                1 => {
                    let normalized = (chunk[0] as f32 - 128.0) / 128.0;
                    let amplified = normalized * self.gain;
                    let clamped = amplified.clamp(-1.0, 1.0);
                    chunk[0] = ((clamped * 128.0) + 128.0) as u8;
                },
                // Similar pattern for 2,3,4 bytes...
                _ => {}
            }
        }
        
        buf[..bytes_read].copy_from_slice(&temp_buf[..bytes_read]);
        Ok(bytes_read)
    }
}

impl<'a, R: ReaderElement> embedded_io::ErrorType for GainReader<'a, R> {
    type Error = R::Error;
}

impl<'a, R: ReaderElement> ReaderElement for GainReader<'a, R> {
    fn get_info(&self) -> Info {
        self.reader.get_info()
    }

    fn available(&self) -> u32 {
        self.reader.available()
    }
}

impl<'a, W: WriterElement> embedded_io::ErrorType for GainWriter<'a, W> {
    type Error = W::Error;
}

impl<'a, W: WriterElement> Write for GainWriter<'a, W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        let mut local_buf = vec![0u8; buf.len()];
        local_buf.copy_from_slice(buf);
        
        // Process as i16 samples
        for chunk in local_buf.chunks_mut(2) {
            if chunk.len() == 2 {
                let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                let normalized = sample as f32 / 32768.0;
                let amplified = normalized * self.gain;
                let clamped = amplified.clamp(-1.0, 1.0);
                let processed = (clamped * 32768.0) as i16;
                let bytes = processed.to_le_bytes();
                chunk.copy_from_slice(&bytes);
            }
        }
        
        self.writer.write(&local_buf)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.writer.flush()
    }
}

impl<'a, W: WriterElement> WriterElement for GainWriter<'a, W> {
    fn get_info(&self) -> Info {
        self.writer.get_info()
    }

    fn available(&self) -> u32 {
        self.writer.available()
    }
}