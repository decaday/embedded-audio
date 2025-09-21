#![cfg_attr(not(feature = "std"), no_std)]

pub mod fmt;

pub mod encoder;
pub mod decoder;
pub mod generator;
pub mod stream;

pub mod transformer;

pub use rivulets::databus;
pub use rivulets::utils;

// pub mod pipeline;
// use std::sync::Arc;
// use embassy_time::{Duration, Timer};


cfg_if::cfg_if! {
    if #[cfg(feature = "std")] {
        pub use std::sync::Mutex;
        
        pub use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex as RawMutex;
    } else {
        pub type Mutex<T> = embassy_sync::blocking_mutex::CriticalSectionMutex<T>;
        
        pub use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex as RawMutex;
    }
}

pub type Channel<T, const N: usize> = embassy_sync::channel::Channel<RawMutex, T, N>;
pub type ChannleReceiver<'a, T, const N: usize> = embassy_sync::channel::Receiver<'a, RawMutex, T, N>;
pub type ChannelSender<'a, T, const N: usize> = embassy_sync::channel::Sender<'a, RawMutex, T, N>;

pub type Signal<T> = embassy_sync::signal::Signal<RawMutex, T>;