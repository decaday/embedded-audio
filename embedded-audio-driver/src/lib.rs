pub mod stream;
pub mod decoder;
pub mod encoder;
pub mod element;
pub mod reader;
pub mod writer;
pub mod info;
pub mod transform;

#[derive(Debug)]
pub enum Error {
    /// Invalid parameters provided
    InvalidParameter,
    /// Device is not initialized
    NotInitialized,
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

pub type Result<T> = core::result::Result<T, Error>;