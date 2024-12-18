use crate::Result;
use crate::stream::{AudioFormat, Stream, OutputStream};

/// DAC channel selection
#[derive(Debug, Clone, Copy)]
pub enum DacChannel {
    Channel1,   // Single channel 1
    Channel2,   // Single channel 2
    Both,       // Dual channel mode
}

/// DAC configuration parameters
#[derive(Debug, Clone)]
pub struct DacConfig {
    pub format: AudioFormat,
    pub channel: DacChannel,
    /// Reference voltage in millivolts
    pub vref: u32,
}

/// Digital-to-Analog Converter interface
/// 
/// This trait defines the operations for DAC devices,
/// supporting both single sample and block data conversion.
pub trait Dac: Stream + OutputStream {
    /// Configure the DAC
    fn configure(&mut self, config: &DacConfig) -> Result<()>;
    
    /// Write a single sample to DAC
    fn write_sample(&mut self, sample: u16) -> Result<()>;
    
    /// Get current DAC configuration
    fn get_config(&self) -> Option<DacConfig>;
}