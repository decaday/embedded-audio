use embedded_io::{Read, Write, Seek};

use crate::info::Info;
use crate::port::{InPort, OutPort, PortRequirement};

#[allow(async_fn_in_trait)]
pub trait Element {
    type Error;

    fn get_in_info(&self) -> Option<Info>;

    fn get_out_info(&self) -> Option<Info>;

    fn get_in_port_requriement(&self) -> PortRequirement;

    fn get_out_port_requriement(&self) -> PortRequirement;

    fn available(&self) -> u32;

    async fn process<R, W>(&mut self, in_port: &mut InPort<'_, R>, out_port: &mut OutPort<'_, W>) -> Result<(), Self::Error>
    where
        R: Read + Seek,
        W: Write + Seek;

}
