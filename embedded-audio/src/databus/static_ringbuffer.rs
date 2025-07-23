use core::panic;
use std::io::{Read, Write};

use embedded_audio_driver::info::Info;
use ringbuf::{traits::Observer, StaticRb};
pub struct StaticRingBuffer<const N: usize> {
    inner: StaticRb<u8, N>,
    info: Option<Info>
}

impl<const N: usize> StaticRingBuffer<N> {
    pub fn new() -> Self {
        Self {
            inner: StaticRb::default(),
            info: None,
        }
    }

    pub fn set_info(&mut self, info: Info) {
        self.info = Some(info);
    }
}


