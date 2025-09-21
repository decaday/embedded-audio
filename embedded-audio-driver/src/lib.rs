#![cfg_attr(not(feature = "std"), no_std)]

pub mod stream;
// pub mod decoder;
// pub mod encoder;
pub mod element;
pub mod info;
pub use rivulets_driver::port;
pub use rivulets_driver::databus;
pub use rivulets_driver::payload;


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
