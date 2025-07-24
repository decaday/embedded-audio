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

    async fn process<'a, R, W, DI, DO>(&mut self, in_port: &mut InPort<'a, R, DI>, out_port: &mut OutPort<'a, W, DO>) -> Result<(), Self::Error>
    where
        R: Read + Seek,
        W: Write + Seek,
        DI: Databus<'a>,
        DO: Databus<'a>;
}
