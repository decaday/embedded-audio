use core::panic;
use std::io::{Read, Write};

use ringbuf::{traits::Observer, StaticRb};
use embedded_audio_driver::element::{ReaderElement, WriterElement, Element};

pub struct StaticRingBuffer<const N: usize> {
    inner: StaticRb<u8, N>
}

impl<const N: usize> StaticRingBuffer<N> {
    pub fn new() -> Self {
        Self {
            inner: StaticRb::default()
        }
    }
}

impl<const N: usize> embedded_io::ErrorType for StaticRingBuffer<N> {
    type Error = core::convert::Infallible;
}

impl<const N: usize> embedded_io::Read for StaticRingBuffer<N> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        Ok(self.inner.read(buf).unwrap())
    }
}

impl<const N: usize> ReaderElement for StaticRingBuffer<N> {
    fn get_info(&self) -> embedded_audio_driver::info::Info {
        todo!()
    }

    fn available(&self) -> u32 {
        self.inner.occupied_len() as _
    }
}

impl<const N: usize> embedded_io::Write for StaticRingBuffer<N> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        Ok(self.inner.write(buf).unwrap())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        todo!()
    }
}

impl<const N: usize> WriterElement for StaticRingBuffer<N> {
    fn get_info(&self) -> embedded_audio_driver::info::Info {
        todo!()
    }

    fn available(&self) -> u32 {
        self.inner.vacant_len() as _
    }
}

impl<const N: usize> Element for StaticRingBuffer<N> {
    type Error = core::convert::Infallible;

    fn get_in_info(&self) -> Option<embedded_audio_driver::info::Info> {
        todo!()
    }

    fn get_out_info(&self) -> Option<embedded_audio_driver::info::Info> {
        todo!()
    }

    fn process<R, W>(&mut self, _reader: Option<&mut R>, _writer: Option<&mut W>) -> Result<(), Self::Error>
    where 
        R: ReaderElement,
        W: WriterElement {
        // This method is not implemented, as the ring buffer does not process audio data directly.
        panic!("StaticRingBuffer does not support process method");
    }
}