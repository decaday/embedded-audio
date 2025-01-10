use embedded_io::Read;

pub trait PullReader: Read {
    fn pull(&mut self) -> Option<u8>;
}