use crate::payload::{Metadata, Payload};

#[allow(async_fn_in_trait)]
pub trait Databus<'b>: Sized {
    /// Asynchronously acquire a payload for reading data
    async fn acquire_read(&'b self) -> Payload<'b, Self> where Self: 'b;

    /// Asynchronously acquire a payload for writing data
    async fn acquire_write(&'b self) -> Payload<'b, Self> where Self: 'b;

    /// Release the payload back to the databus
    fn release(&self, buf: &'b mut [u8], metadata: Metadata, is_write: bool);
}
