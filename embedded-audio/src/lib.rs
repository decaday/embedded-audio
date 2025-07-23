pub mod encoder;
pub mod decoder;
pub mod databus;
// pub mod stream;
// pub mod pipeline;
// pub mod generator;
// pub mod utils;
// pub mod ringbuffer;
// pub mod transform;

// use std::sync::Arc;
// use embassy_time::{Duration, Timer};
// use cpal::{Sample};
// use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
// use ringbuffer::AllocRingBuffer as RingBuffer;

cfg_if::cfg_if! {
    if #[cfg(feature = "std")] {
        pub use std::sync::Mutex;
    } else {
        pub type Mutex<T> = embassy_sync::blocking_mutex::CriticalSectionMutex<T>;
    }
}
