use embedded_io::{Read, Write, Seek};

use crate::databus::{Databus, Release}; 
use crate::payload::{Metadata, Payload};
use crate::slot::{SlotConsumer, SlotProducer};

pub enum InPort<'a, R: Read + Seek, T: Release<'a>, D: Databus<'a, T>> {
    Reader(&'a mut R),
    Payload(&'a D),
    None,
    _Phantom(core::marker::PhantomData<T>),
}

pub enum OutPort<'a, W: Write + Seek, T: Release<'a>, D: Databus<'a, T>> {
    Writer(&'a mut W),
    Payload(&'a D),
    None,
    _Phantom(core::marker::PhantomData<T>),
}

pub enum PortRequirement {
    None,
    /// Writer + Seek or Reader + Seek
    IO,
    /// payload min size
    Payload(u32),
}

/// Dummy implementation of Databus, Read, Write, and Seek traits
pub struct Dmy;

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

impl<'b> Release<'b> for Dmy {
    fn release(&self, _buf: &'b mut [u8], _metadata: Metadata, _is_write: bool) {
        unimplemented!()
    }
}

impl<'b> Databus<'b, Dmy> for Dmy {
    async fn acquire_read(&'b self) -> Payload<'b, Dmy> where Dmy: 'b {
        unimplemented!()
    }

    async fn acquire_write(&'b self) -> Payload<'b, Dmy> where Dmy: 'b {
        unimplemented!()
    }
}
