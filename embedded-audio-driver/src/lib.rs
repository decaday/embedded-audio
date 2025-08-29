pub mod stream;
// pub mod decoder;
// pub mod encoder;
pub mod element;
pub mod info;
// pub mod transform;
pub mod port;
pub mod databus;
pub mod payload;

cfg_if::cfg_if! {
    if #[cfg(feature = "std")] {
        pub use std::sync::Mutex;
        pub use std::sync::MutexGuard;
    } else {
        pub type Mutex<T> = embassy_sync::blocking_mutex::CriticalSectionMutex<T>;
    }
}

#[derive(Debug)]
pub enum Error {
    /// Invalid parameters provided
    InvalidParameter,
    /// Device is not initialized
    NotInitialized,
    /// Device is in an invalid state for the requested operation
    InvalidState,
    /// Device is busy
    Busy,
    /// Operation timed out
    Timeout,
    /// Buffer is full
    BufferFull,
    /// Buffer is empty
    BufferEmpty,
    /// Device hardware error
    DeviceError,
    /// Operation not supported
    Unsupported,
}

// pub type Result<T> = core::result::Result<T, Error>;
