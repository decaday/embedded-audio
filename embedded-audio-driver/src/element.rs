use embedded_io::{Read, Seek, Write};

// Use statements to import all necessary types at the top.
use crate::databus::{Consumer, Producer, Transformer};
use crate::info::Info;
use crate::port::{Dmy, InPlacePort, InPort, OutPort, PortRequirements};

/// The core trait for any audio processing unit in the pipeline.
///
/// An `Element` can be a source (generator), a sink (encoder, output stream),
/// or a transformation (filter, gain). It processes data by receiving it from
/// an `InPort` and sending it to an `OutPort`.
#[allow(async_fn_in_trait)]
pub trait Element {
    // TODO
    type Error;

    /// Returns the audio format information expected at the input.
    /// Returns `None` if this is a source element.
    fn get_in_info(&self) -> Option<Info>;

    /// Returns the audio format information produced at the output.
    /// Returns `None` if this is a sink element.
    fn get_out_info(&self) -> Option<Info>;

    /// Could be called before `initialize`
    fn need_writer(&self) -> bool {
        false
    }
    /// Could be called before `initialize`
    fn need_reader(&self) -> bool {
        false
    }

    /// MUST be called after `initialize`
    fn get_port_requirements(&self) -> PortRequirements;

    /// Returns the amount of available data for processing.
    /// The unit (e.g., bytes, frames) depends on the element's nature.
    fn available(&self) -> u32;

    async fn initialize<'a, R, W>(
        &mut self,
        in_port: &mut InPort<'a, R, Dmy>,
        out_port: &mut OutPort<'a, W, Dmy>,
        upstream_info: Option<Info>,
    ) -> Result<PortRequirements, Self::Error>
    where
        R: Read + Seek,
        W: Write + Seek
    {
        let _ = in_port;
        let _ = out_port;
        let _ = upstream_info;
        Ok(self.get_port_requirements())
    }

    /// Flushes any internal buffers
    async fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Resets the internal state and info of the element.
    async fn reset(&mut self) -> Result<(), Self::Error> {
        self.flush().await
    }

    /// The main asynchronous processing function.
    ///
    /// It takes an `InPort` and an `OutPort` and performs one step of data
    /// processing. The implementation will match on the port types to handle
    /// different kinds of connections (IO-based or databus-based).
    ///
    /// # In-Place Transformation (Zero-Copy)
    ///
    /// To perform an in-place transformation, an `Element`'s `process` method
    /// should be called with its `InPort::Consumer` and `OutPort::Producer`
    /// ports connected to the *same* databus instance.
    ///
    /// The `Element` implementation can then detect this condition (e.g., by
    /// comparing pointers) and safely cast the databus to the `Transformer` trait
    /// to acquire a mutable payload for in-place modification, thus achieving
    /// zero-copy processing. If the ports are connected to different databuses,
    /// the element should fall back to a copy-based approach.
    async fn process<'a, R, W, C, P, T>(
        &mut self,
        in_port: &mut InPort<'a, R, C>,
        out_port: &mut OutPort<'a, W, P>,
        inplace_port: &mut InPlacePort<'a, T>,
    ) -> ProcessResult<Self::Error>
    where
        R: Read + Seek,
        W: Write + Seek,
        C: Consumer<'a>,
        P: Producer<'a>,
        T: Transformer<'a>;
}

/// Represents the status of a `process` call.
pub enum ProcessStatus {
    /// The processing step completed successfully, and more data may follow.
    Fine,
    /// The element has reached the end of its data stream.
    Eof,
}
pub use ProcessStatus::{Eof, Fine};

/// A convenient type alias for the result of a `process` call.
pub type ProcessResult<E> = Result<ProcessStatus, E>;
