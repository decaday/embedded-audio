use crate::info::Info;
use crate::TrySeek;
use crate::Mutex;

use crate::slot::{InPort, OutPort};

pub trait Element {
    fn get_in_info(&self) -> Option<Info>;

    fn get_out_info(&self) -> Option<Info>;

    fn available(&self) -> u32;

    async fn process(&mut self, in_port: Option<&InPort>, out_port: Option<&OutPort>) -> Result<(), ()>;
}



pub trait PassiveElement: Element {
}

pub trait NegativeElement: Element {
}
