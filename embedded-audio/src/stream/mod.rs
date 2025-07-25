#[cfg(feature = "std")]
pub mod cpal_output;

#[cfg(feature = "std")]
pub use cpal_output::CpalOutputStream;