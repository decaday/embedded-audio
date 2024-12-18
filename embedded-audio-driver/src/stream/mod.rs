use crate::Result;

pub mod i2s;
pub mod dac;

/// Audio format configuration
#[derive(Debug, Clone, Copy)]
pub struct AudioFormat {
    /// Sampling rate in Hz (e.g., 44100, 48000)
    pub sample_rate: u32,
    /// Bits per sample (e.g., 16, 24, 32)
    pub bits_per_sample: u8,
    /// Number of channels (1 for mono, 2 for stereo)
    pub channels: u8,
}

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
    fn init(&mut self, format: &AudioFormat) -> Result<()>;
    
    /// Start the stream
    fn start(&mut self) -> Result<()>;
    
    /// Stop the stream and reset internal state
    fn stop(&mut self) -> Result<()>;
    
    /// Pause the stream (maintains internal state)
    fn pause(&mut self) -> Result<()>;
    
    /// Resume a paused stream
    fn resume(&mut self) -> Result<()>;
    
    /// Get current stream state
    fn state(&self) -> StreamState;
    
    /// Get current audio format configuration
    fn format(&self) -> Option<AudioFormat>;
}

/// Input stream interface for audio capture
/// 
/// This trait defines operations for audio input streams,
/// such as microphones, ADC, or file readers.
pub trait InputStream: Stream {
    /// Read audio data into the provided buffer
    /// Returns the number of bytes read
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize>;
    
    /// Check if data is available for reading
    /// Returns the number of bytes available
    fn available(&self) -> Result<usize>;
}

/// Output stream interface for audio playback
/// 
/// This trait defines operations for audio output streams,
/// such as speakers, DAC, or file writers.
pub trait OutputStream: Stream {
    /// Write audio data from the provided buffer
    /// Returns the number of bytes written
    fn write(&mut self, buffer: &[u8]) -> Result<usize>;
    
    /// Check if the stream can accept more data
    /// Returns the number of bytes that can be written
    fn space_available(&self) -> Result<usize>;
}