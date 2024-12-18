use crate::Result;
use crate::stream::{AudioFormat, Stream, InputStream, OutputStream};

/// I2S operating modes
#[derive(Debug, Clone, Copy)]
pub enum I2sMode {
    Master,  // Clock provider
    Slave,   // Clock receiver
}

/// I2S protocol standards
#[derive(Debug, Clone, Copy)]
pub enum I2sStandard {
    Philips,    // Standard I2S format
    MSB,        // MSB justified format
    LSB,        // LSB justified format
    PCM,        // PCM format
}

/// I2S configuration parameters
#[derive(Debug, Clone)]
pub struct I2sConfig {
    pub mode: I2sMode,
    pub standard: I2sStandard,
    pub format: AudioFormat,
    /// MCLK divider ratio (optional)
    pub mclk_div: Option<u32>,
}

/// I2S device interface
/// 
/// This trait defines the operations for I2S (Inter-IC Sound) devices,
/// supporting both transmission and reception of audio data.
pub trait I2s: Stream {
    /// Configure the I2S interface
    fn configure(&mut self, config: &I2sConfig) -> Result<()>;
    
    /// Get current I2S configuration
    fn get_config(&self) -> Option<I2sConfig>;
}

/// I2S input device interface
pub trait I2sInput: I2s + InputStream {}

/// I2S output device interface
pub trait I2sOutput: I2s + OutputStream {}