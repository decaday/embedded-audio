pub use rivulets_driver::element::Element as BaseElement;
pub use rivulets_driver::element::{ProcessResult, ProcessStatus, Eof, Fine};

use crate::info::Info;

/// A specialized `Element` for audio processing pipelines.
///
/// This trait fixes the `Info` associated type to `AudioInfo`, providing a
/// common interface for all audio components.
pub trait Element: BaseElement<Info = Info> {}

impl<T> Element for T where T: BaseElement<Info = Info> {}