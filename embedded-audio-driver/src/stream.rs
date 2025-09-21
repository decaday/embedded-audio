//! Defines a specialized `Stream` trait for audio endpoints.

use crate::element::Element;
pub use rivulets_driver::stream::Stream as BaseStream;
pub use rivulets_driver::stream::{Error, StreamState};

/// A specialized `Stream` for audio sources and sinks.
///
/// This trait combines the `AudioElement` and `rivulets_driver::stream::Stream`
/// traits for components that act as audio pipeline endpoints.
pub trait Stream: BaseStream + Element {}

// Blanket implementation for convenience.
impl<T> Stream for T where T: BaseStream + Element {}