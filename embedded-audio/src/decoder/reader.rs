use embedded_audio_driver::{decoder::Decoder, element::Element};
use embedded_audio_driver::reader;
use embedded_io::{ErrorType, Read};
pub struct DecoderReader<'a, T: Decoder> {
    decoder: &'a mut T,
}

impl<'a, T: Decoder> DecoderReader<'a, T> {
    pub fn new(decoder: &'a mut T) -> Self {
        Self { decoder }
    }
}

impl<'a, T: Decoder> ErrorType for DecoderReader<'a, T> {
    type Error = reader::Error;
}

impl<'a, T: Decoder> Read for DecoderReader<'a, T> {

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.decoder.read(buf).map_err(reader::Error::from)
    }
    
    fn read_exact(&mut self, mut buf: &mut [u8]) -> Result<(), embedded_io::ReadExactError<Self::Error>> {
        while !buf.is_empty() {
            match self.read(buf) {
                Ok(0) => break,
                Ok(n) => buf = &mut buf[n..],
                Err(e) => return Err(embedded_io::ReadExactError::Other(e)),
            }
        }
        if buf.is_empty() {
            Ok(())
        } else {
            Err(embedded_io::ReadExactError::UnexpectedEof)
        }
    }
}

impl<T: Decoder> Element for DecoderReader<'_, T> {
    type Error = reader::Error;

    fn get_in_info(&self) -> Option<embedded_audio_driver::info::Info> {
        None
    }

    fn get_out_info(&self) -> Option<embedded_audio_driver::info::Info> {
        Some(self.decoder.get_info())
    }
}