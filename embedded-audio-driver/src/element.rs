use embedded_io::{Read, Write, Seek};

use crate::databus::{Databus, Release};
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

    async fn process<'a, R, W, T, D>(&mut self, in_port: &mut InPort<'a, R, T, D>, out_port: &mut OutPort<'a, W, T, D>) -> Result<(), Self::Error>
    where
        R: Read + Seek,
        W: Write + Seek,
        T: Release<'a>,
        D: Databus<'a, T>;
}
