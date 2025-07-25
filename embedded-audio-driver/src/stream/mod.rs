use crate::element::Element;

// pub mod i2s;
// pub mod dac;

/// Stream errors
#[derive(Debug)]
pub enum Error {
    // TODO:
    // Custom(E),
    Unsupported,
    Timeout,
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
pub trait Stream: Element {
    /// Start the stream
    fn start(&mut self) -> Result<(), Self::Error>;
    
    /// Stop the stream and reset internal state
    fn stop(&mut self) -> Result<(), Self::Error>;
    
    /// Pause the stream (maintains internal state)
    fn pause(&mut self) -> Result<(), Self::Error>;
    
    /// Resume a paused stream
    fn resume(&mut self) -> Result<(), Self::Error>;
    
    /// Get current stream state
    fn get_state(&self) -> StreamState;
}