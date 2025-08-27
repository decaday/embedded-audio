use embedded_io::{Read, Seek, Write};

use crate::databus::{Consumer, Producer, Transformer};
use crate::payload::{Metadata, ReadPayload, TransformPayload, WritePayload};

/// Represents an input port for an `Element`.
///
/// An `Element` can receive data either from a standard `Read + Seek` source
/// or from a `Consumer`, which provides data payloads.
pub enum InPort<'a, R: Read + Seek, C: Consumer<'a>> {
    /// An input source that implements the `Read` and `Seek` traits.
    Reader(&'a mut R),
    /// An upstream databus component that implements the `Consumer` trait.
    Consumer(&'a C),
    /// Represents no input, typically for source elements like generators.
    None,
}

impl<'a, R: Read + Seek> InPort<'a, R, Dmy> {
    pub fn new_reader(reader: &'a mut R) -> Self {
        InPort::Reader(reader)
    }
}

impl InPort<'_, Dmy, Dmy> {
    pub fn new_none() -> Self {
        InPort::None
    }
}

/// Represents an output port for an `Element`.
///
/// An `Element` can send data either to a standard `Write + Seek` sink
/// or to a `Producer`, which accepts data payloads.
pub enum OutPort<'a, W: Write + Seek, P: Producer<'a>> {
    /// An output sink that implements the `Write` and `Seek` traits.
    Writer(&'a mut W),
    /// A downstream databus component that implements the `Producer` trait.
    Producer(&'a P),
    /// Represents no output, typically for sink elements.
    None,
}

impl<'a, W: Write + Seek> OutPort<'a, W, Dmy> {
    pub fn new_writer(writer: &'a mut W) -> Self {
        OutPort::Writer(writer)
    }
}

impl OutPort<'_, Dmy, Dmy> {
    pub fn new_none() -> Self {
        OutPort::None
    }
}

/// Represents an in-place transformation port for an `Element`.
///
/// An `Element` can perform in-place transformations using a `Transformer`.
/// This is typically used for effects or filters that modify data in place.
pub enum InPlacePort<'a, T: Transformer<'a>> {
    /// An upstream databus component that implements the `Transformer` trait.
    Transformer(&'a T),
    /// Represents no in-place transformation, typically for elements that do not modify data in place.
    None,
}

impl InPlacePort<'_, Dmy> {
    pub fn new_none() -> Self {
        InPlacePort::None
    }
}

/// The data transfer requirements for an `Element`'s port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortRequirements {
    pub writer: bool,
    pub reader: bool,
    pub in_payload: Option<u16>,
    pub out_payload: Option<u16>,
    pub in_place: Option<u16>,
}

impl PortRequirements {
    pub fn new() -> Self {
        Self {
            writer: false,
            reader: false,
            in_payload: None,
            out_payload: None,
            in_place: None,
        }
    }

    pub fn new_reader_to_payload(size: u16) -> Self {
        Self {
            writer: false,
            reader: true,
            in_payload: Some(size),
            out_payload: None,
            in_place: None,
        }
    }

    pub fn new_payload_to_writer(size: u16) -> Self {
        Self {
            writer: true,
            reader: false,
            in_payload: None,
            out_payload: Some(size),
            in_place: None,
        }
    }

    pub fn new_payload_to_payload(in_size: u16, out_size: u16) -> Self {
        Self {
            writer: true,
            reader: true,
            in_payload: Some(in_size),
            out_payload: Some(out_size),
            in_place: None,
        }
    }

    pub fn new_in_place(size: u16) -> Self {
        Self {
            writer: false,
            reader: true,
            in_payload: None,
            out_payload: None,
            in_place: Some(size),
        }
    }

    pub fn sink(size: u16) -> Self {
        Self {
            writer: false,
            reader: false,
            in_payload: Some(size),
            out_payload: None,
            in_place: None,
        }
    }

    pub fn source(size: u16) -> Self {
        Self {
            writer: false,
            reader: false,
            in_payload: None,
            out_payload: Some(size),
            in_place: None,
        }
    }
    
    pub fn need_writer(&self) -> bool {
        self.writer
    }

    pub fn need_reader(&self) -> bool {
        self.reader
    }
}

/// A dummy struct used as a placeholder for unused generic type parameters
/// in `InPort` and `OutPort` when a port is not of a databus or IO type.
pub struct Dmy;

// --- Dummy Trait Implementations for Dmy ---

impl embedded_io::ErrorType for Dmy {
    type Error = core::convert::Infallible;
}

impl Read for Dmy {
    fn read(&mut self, _buf: &mut [u8]) -> Result<usize, Self::Error> {
        unimplemented!()
    }
}

impl Write for Dmy {
    fn write(&mut self, _buf: &[u8]) -> Result<usize, Self::Error> {
        unimplemented!()
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        unimplemented!()
    }
}

impl Seek for Dmy {
    fn seek(&mut self, _pos: embedded_io::SeekFrom) -> Result<u64, Self::Error> {
        unimplemented!()
    }
}

// Dmy needs to implement the databus traits to be used as a generic placeholder.
// These implementations will panic if ever called.
impl<'a> Consumer<'a> for Dmy {
    async fn acquire_read(&'a self) -> ReadPayload<'a, Self> {
        unimplemented!()
    }
    fn release_read(&self, _consumed_bytes: usize) {
        unimplemented!()
    }
}

impl<'a> Producer<'a> for Dmy {
    async fn acquire_write(&'a self) -> WritePayload<'a, Self> {
        unimplemented!()
    }
    fn release_write(&self, _buf: &'a mut [u8], _metadata: Metadata) {
        unimplemented!()
    }
}

// The dummy implementation must also include the Transformer trait.
impl<'a> Transformer<'a> for Dmy {
    async fn acquire_transform(&'a self) -> TransformPayload<'a, Self> {
        unimplemented!()
    }
    fn release_transform(&self, _buf: &'a mut [u8], _metadata: Metadata) {
        unimplemented!()
    }
}
