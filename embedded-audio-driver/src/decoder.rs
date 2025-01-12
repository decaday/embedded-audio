use embedded_io::ReadExactError;

use crate::element::Info;

/// Decoder runtime state
#[derive(Debug, Clone, Copy)]
pub struct DecoderState {
    /// Number of samples decoded so far
    pub decoded_samples: u64,
    // /// Current bitrate in bits per second
    // pub current_bitrate: u32,
}

/// Audio decoder interface
/// 
/// This trait defines the operations for audio decoders,
/// supporting initialization, decoding, and state management.
pub trait Decoder {
    /// Initialize the decoder
    fn init(&mut self) -> Result<(), Error>;
    
    /// Read audio data
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Error>;
    
    /// Get decoder capabilities and information
    fn get_info(&self) -> Info;
    
    /// Get current decoder state
    fn get_state(&self) -> Result<DecoderState, Error>;
    
    fn seek(&mut self, sample_num: u64) -> Result<(), Error>;
}

#[derive(Debug)]
pub enum Error {
    Io(embedded_io::ErrorKind),
    UnexpectedEof,
    InvalidHeader,
    UnsupportedFormat,
    BufferOverflow,
    InvalidData,
    UnsupportedFunction,
    // Other,
}

impl Error {
    pub fn from_io<E: embedded_io::Error>(err: E) -> Error {
        Error::Io(err.kind())
    }

    pub fn from_io_read_exact<E: embedded_io::Error>(err: ReadExactError<E>) -> Error {
        match err {
            ReadExactError::UnexpectedEof => Error::UnexpectedEof,
            ReadExactError::Other(e) => Error::Io(e.kind()),
        }
    }
}

impl embedded_io::Error for Error {
    fn kind(&self) -> embedded_io::ErrorKind {
        match self {
            Error::Io(kind) => *kind,
            _ => embedded_io::ErrorKind::Other,
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(kind) => write!(f, "IO error: {:?}", kind),
            Error::UnexpectedEof => write!(f, "Unexpected EOF"),
            Error::InvalidHeader => write!(f, "Invalid header"),
            Error::UnsupportedFormat => write!(f, "Unsupported format"),
            Error::BufferOverflow => write!(f, "Buffer overflow"),
            Error::InvalidData => write!(f, "Invalid data"),
            Error::UnsupportedFunction => write!(f, "Unsupported function"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}