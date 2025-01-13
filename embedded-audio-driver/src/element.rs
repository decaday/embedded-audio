// use ringbuffer::AllocRingBuffer as RingBuffer;
use crate::info::Info;
use core::convert::Infallible;

pub trait Element {
    type Error;

    fn get_in_info(&self) -> Option<Info>;

    fn get_out_info(&self) -> Option<Info>;

    // fn progress(&mut self, in_ringbuffer: &mut RingBuffer<u8>, out_ringbuffer: &mut RingBuffer<u8>);
}

pub trait ReaderElement {
    fn init(&mut self) -> Result<(), Infallible>;

    fn get_info(&self) -> Info;
    
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Infallible>;
}