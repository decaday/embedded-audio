use embedded_io::{Read, Write, Seek};

use crate::databus::Databus; 
use crate::payload::{Metadata, Payload};

pub enum InPort<'a, R: Read + Seek, D: Databus<'a>> {
    Reader(&'a mut R),
    Payload(&'a D),
    None,
}

pub enum OutPort<'a, W: Write + Seek, D: Databus<'a>> {
    Writer(&'a mut W),
    Payload(&'a D),
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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


impl<'b> Databus<'b> for Dmy {
    async fn acquire_read(&'b self) -> Payload<'b, Dmy> where Dmy: 'b {
        unimplemented!()
    }

    async fn acquire_write(&'b self) -> Payload<'b, Dmy> where Dmy: 'b {
        unimplemented!()
    }
    
    fn release(&self, _buf: &'b mut [u8], _metadata: Metadata, _is_write: bool) {
        unimplemented!()
    }
}
