pub mod stream;
pub mod decoder;
pub mod encoder;
pub mod element;
pub mod info;
pub mod transform;
pub mod slot;

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

pub mod try_seek {
    use embedded_io::SeekFrom;
    pub struct Unsupported {}

    pub trait TrySeek {
        type Error: core::fmt::Debug;

        /// Attempts Seek to an offset, in bytes, in a stream.
        fn try_seek(&mut self, _pos: SeekFrom) -> Result<Result<u64, Self::Error>, Unsupported> {
            Err(Unsupported {})
        }

        /// Attempts to rewind the stream to the beginning.
        fn try_rewind(&mut self) -> Result<Result<(), Self::Error>, Unsupported> {
            Err(Unsupported {})
        }

        /// Attempts to get the current position in the stream.
        fn stream_position(&mut self) -> Result<Result<u64, Self::Error>, Unsupported> {
            Err(Unsupported {})
        }
    }

    impl<T: embedded_io::Seek> TrySeek for T {
        type Error = T::Error;
        fn try_seek(&mut self, pos: SeekFrom) -> Result<Result<u64, Self::Error>, Unsupported> {
            Ok(self.seek(pos))
        }

        fn try_rewind(&mut self) -> Result<Result<(), Self::Error>, Unsupported> {
            Ok(self.rewind())
        }

        fn stream_position(&mut self) -> Result<Result<u64, Self::Error>, Unsupported> {
            Ok(self.stream_position())
        }
    }
}

pub use try_seek::TrySeek;
