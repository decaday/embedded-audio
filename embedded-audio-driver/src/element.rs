use crate::info::Info;

pub trait WriterElement: embedded_io::Write {
    fn get_info(&self) -> Info;

    fn available(&self) -> u32;
}

pub trait ReaderElement: embedded_io::Read {
    fn get_info(&self) -> Info;

    fn available(&self) -> u32;
}

pub trait Element {
    type Error: core::fmt::Debug;

    fn get_in_info(&self) -> Option<Info>;

    fn get_out_info(&self) -> Option<Info>;

    fn process<R, W>(&mut self, reader: Option<&mut R>, writer: Option<&mut W>) -> Result<(), Self::Error>
    where 
        R: ReaderElement,
        W: WriterElement;
}