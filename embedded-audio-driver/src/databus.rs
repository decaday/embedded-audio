use crate::payload::{Metadata, Payload};

#[allow(async_fn_in_trait)]
pub trait Databus<'b, T: Release<'b>> {
    /// Asynchronously acquire a payload for reading data
    async fn acquire_read(&'b self) -> Payload<'b, T> where T: 'b;

    /// Asynchronously acquire a payload for writing data
    async fn acquire_write(&'b self) -> Payload<'b, T> where T: 'b;
}

pub trait Release<'b> {
    /// Release the payload back to the databus
    fn release(&self, buf: &'b mut [u8], metadata: Metadata, is_write: bool);
}