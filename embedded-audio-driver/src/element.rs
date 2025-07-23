use embedded_io::{Read, Write, Seek};

use crate::databus::Databus;
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

    async fn process<'a, R, W,  D>(&mut self, in_port: &mut InPort<'a, R, D>, out_port: &mut OutPort<'a, W, D>) -> Result<(), Self::Error>
    where
        R: Read + Seek,
        W: Write + Seek,
        D: Databus<'a>;
}
