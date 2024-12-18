use crate::Result;
use crate::stream::AudioFormat;

/// Decoder information and capabilities
#[derive(Debug, Clone)]
pub struct DecoderInfo {
    /// List of supported input formats (e.g., "MP3", "WAV")
    pub supported_formats: &'static [&'static str],
    /// Output audio format specification
    pub output_format: AudioFormat,
}

/// Decoder runtime state
#[derive(Debug, Clone, Copy)]
pub struct DecoderState {
    /// Total number of samples in the stream
    pub total_samples: u64,
    /// Number of samples decoded so far
    pub decoded_samples: u64,
    /// Current bitrate in bits per second
    pub current_bitrate: u32,
}

/// Audio decoder interface
/// 
/// This trait defines the operations for audio decoders,
/// supporting initialization, decoding, and state management.
pub trait Decoder {
    /// Initialize the decoder
    fn init(&mut self) -> Result<()>;
    
    /// Decode a block of input data
    /// Returns the number of bytes written to output
    fn decode(&mut self, input: &[u8], output: &mut [u8]) -> Result<usize>;
    
    /// Get decoder capabilities and information
    fn get_info(&self) -> DecoderInfo;
    
    /// Get current decoder state
    fn get_state(&self) -> Result<DecoderState>;
    
    /// Reset decoder to initial state
    fn reset(&mut self) -> Result<()>;
}