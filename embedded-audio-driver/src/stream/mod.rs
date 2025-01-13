use crate::info::Info;


pub mod i2s;
pub mod dac;

/// Stream states
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StreamState {
    Uninitialized,
    Initialized,
    Running,
    Paused,
    Stopped,
}

/// Common stream operations
pub trait Stream {
    /// Initialize the stream with specified format
    fn init(&mut self) -> Result<(), Error>;
    
    /// Start the stream
    fn start(&mut self) -> Result<(), Error>;
    
    /// Stop the stream and reset internal state
    fn stop(&mut self) -> Result<(), Error>;
    
    /// Pause the stream (maintains internal state)
    fn pause(&mut self) -> Result<(), Error>;
    
    /// Resume a paused stream
    fn resume(&mut self) -> Result<(), Error>;
    
    /// Get current stream state
    fn get_state(&self) -> StreamState;

    /// Get stream information
    fn get_info(&self) -> Info;
}

/// Input stream interface for audio capture
/// 
/// This trait defines operations for audio input streams,
/// such as microphones, ADC, or file readers.
pub trait InputStream: Stream {
    /// Read audio data into the provided buffer
    /// Returns the number of bytes read
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Error>;
    
    /// Check if data is available for reading
    /// Returns the number of bytes available
    fn available(&self) -> Result<usize, Error>;

    #[cfg(feature = "async")]
    async fn available(&mut self, buffer: &mut [u8]) -> Result<usize, Error>;
}

/// Output stream interface for audio playback
/// 
/// This trait defines operations for audio output streams,
/// such as speakers, DAC, or file writers.
pub trait OutputStream: Stream {
    /// Write audio data from the provided buffer
    /// Returns the number of bytes written
    fn write(&mut self, buffer: &[u8]) -> Result<usize, Error>;
    
    /// Check if the stream can accept more data
    /// Returns the number of bytes that can be written
    fn space_available(&self) -> Result<usize, Error>;
}

/// Stream errors
#[derive(Debug)]
pub enum Error {
    // TODO:
    // Custom(E),
    Unsupported,
    Timeout,
}