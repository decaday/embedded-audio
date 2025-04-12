use crate::info::Info;
use crate::element::{Element, ReaderElement, WriterElement};

pub trait Transform {
    type Error;

    fn get_in_info(&self) -> Info;

    fn get_out_info(&self) -> Info;

    fn get_min_frame_num(&self) -> usize {
        1
    }

    fn transform(&mut self, buffer: &mut [u8]) -> Result<(), Self::Error>;

    // fn get_reader<R: ReaderElement>(&mut self, reader: &mut R) -> impl ReaderElement;

    // fn get_writer<W: WriterElement>(&mut self, writer: &mut W) -> impl WriterElement;
}
