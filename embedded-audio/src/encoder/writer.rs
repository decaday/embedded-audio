use embedded_audio_driver::encoder::Encoder;
use embedded_audio_driver::writer;
use embedded_io::{ErrorType, Write};

pub struct EncoderWriter<'a, T: Encoder> {
    encoder: &'a mut T,
}

impl<'a, T: Encoder> ErrorType for EncoderWriter<'a, T> {
    type Error = writer::Error;
}

impl<'a, T: Encoder> Write for EncoderWriter<'a, T> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.encoder.write(buf).map_err(writer::Error::from)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        // No-op for now, as flushing might not be necessary for all encoders
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_io_adapters::std::FromStd;
    use std::io::Cursor;
    use crate::encoder::wav::WavEncoder;
    use embedded_audio_driver::element::Info;

    #[test]
    fn test_encoder_writer() {
        let mut buffer = Vec::new();
        let mut cursor = FromStd::new(Cursor::new(&mut buffer));
        let info = Info {
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: 16,
            duration: None,
            num_frames: None,
        };
        let mut encoder = WavEncoder::new(&mut cursor, info).expect("Failed to create WavEncoder");
        let mut writer = EncoderWriter { encoder: &mut encoder };

        let samples = vec![0u8; 1024];
        let bytes_written = writer.write(&samples).expect("Failed to write samples");

        assert_eq!(bytes_written, 1024);
    }
}