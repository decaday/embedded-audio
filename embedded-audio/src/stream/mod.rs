#[cfg(feature = "std")]
pub mod cpal_stream;

#[cfg(feature = "std")]
pub use cpal_stream::CpalOutputStream;