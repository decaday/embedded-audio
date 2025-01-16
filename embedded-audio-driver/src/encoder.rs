use crate::element::WriterElement;

/// Encoder runtime state
#[derive(Debug, Clone, Copy)]
pub struct EncoderState {
    /// Number of samples encoded so far
    pub encoded_samples: u64,
    // /// Current bitrate in bits per second
    // pub current_bitrate: u32,
}

/// Audio encoder interface
/// 
/// This trait defines the operations for audio encoders,
/// supporting initialization, encoding, and state management.
pub trait Encoder: WriterElement {
    /// Initialize the encoder
    fn init(&mut self) -> Result<(), Error>;
    
    /// Get current encoder state
    fn get_state(&self) -> Result<EncoderState, Error>;

    fn stop(&mut self) -> Result<(), Error>;
}

#[derive(Debug)]
pub enum Error {
    Io(embedded_io::ErrorKind),
    BufferUnderflow,
    InvalidConfig,
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
            Error::BufferUnderflow => write!(f, "Buffer underflow"),
            Error::InvalidConfig => write!(f, "Invalid configuration"),
            Error::UnsupportedFormat => write!(f, "Unsupported format"),
            Error::BufferOverflow => write!(f, "Buffer overflow"),
            Error::InvalidData => write!(f, "Invalid data"),
            Error::UnsupportedFunction => write!(f, "Unsupported function"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}