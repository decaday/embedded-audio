use crate::relay::{self, Relay};
use embedded_audio_driver::decoder::Decoder;
use embedded_audio_driver::element::{Element, ReaderElement, WriterElement};
use embedded_audio_driver::encoder::Encoder;
use embedded_audio_driver::stream::{InputStream, OutputStream, Stream};

// struct Pipeline<'a, R> {
//     decoder: decoder::WavDecoder<'a, R>,
//     stream: stream::Stream,
//     ring_buffer: Arc<RingBuffer<u8>>,
// }

/// for no_alloc
pub struct Pipeline1D1OS<D1: Decoder, OS1: OutputStream> {
    pub decoder1: D1,
    pub output_stream1: OS1,
}

/// for no_alloc
pub struct Pipeline1IS1ENC<IS1: InputStream, ENC1: Encoder> {
    pub input_stream1: IS1,
    pub encoder1: ENC1,
}

// pub struct PipelineR2AR2W<R1, W1, E1, E2>
// where
//     R1: Read + Element<Error=E1> + embedded_io::ErrorType<Error=E1>,
//     W1: Write + Element<Error=E2> + embedded_io::ErrorType<Error=E2>,
//     E1: core::fmt::Debug,
//     E2: std::fmt::Debug,
// {
//     pub reader1: R1,
//     pub relay: Relay<R1, W1, E1, E2, 1024>,
//     pub writer1: W1,
// }

// impl<R1, W1, E1, E2> PipelineR2AR2W<R1, W1, E1, E2>
// where
//     R1: Read + Element<Error=E1> + embedded_io::ErrorType<Error=E1>,
//     W1: Write + Element<Error=E2> + embedded_io::ErrorType<Error=E2>,
//     E1: core::fmt::Debug,
//     E2: std::fmt::Debug,
// {
//     pub fn new(reader1: R1, relay: Relay<R1, W1, E1, E2, 1024>, writer1: W1) -> Self {
//         PipelineR2AR2W {
//             reader1,
//             relay,
//             writer1,
//         }
//     }

//     pub fn run(&mut self) -> Result<(), relay::Error<E1, E2>> {
//         self.relay.process()
//     }
// }

pub struct PipelineR2S<R1: ReaderElement, S1: Stream + Element> {
    pub reader_element1: R1,
    pub stream1: S1,
}

impl<R1: ReaderElement, S1: Stream + Element> PipelineR2S<R1, S1> {
    pub fn new(reader_element1: R1, stream1: S1) -> Self {
        PipelineR2S {
            reader_element1,
            stream1,
        }
    }

    pub fn run(&mut self) -> Result<(), ()> {
        // assert!(self.reader_element1.get_info() == self.stream1.get_in_info().unwrap());

        self.stream1.start().unwrap();
        loop {
            self.stream1.process::<R1, DummyWriter>(Some(&mut self.reader_element1), None).unwrap();
        }
        Ok(())
    }
}

pub struct DummyWriter {}
impl embedded_io::ErrorType for DummyWriter {
    type Error = core::convert::Infallible;
}

impl embedded_io::Write for DummyWriter {
    fn write(&mut self, _buf: &[u8]) -> Result<usize, Self::Error> {
        todo!()
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        todo!()
    }
}

impl WriterElement for DummyWriter {
    fn get_info(&self) -> embedded_audio_driver::info::Info {
        todo!()
    }

    fn available(&self) -> u32 {
        todo!()
    }
}