use crate::payload::{Metadata, ReadPayload, TransformPayload, WritePayload};
use crate::port::{Dmy, InPlacePort, InPort, OutPort};

pub enum Operation {
    Read,
    Write,
    InPlace,
}

/// A trait for components from which audio data can be read asynchronously.
#[allow(async_fn_in_trait)]
pub trait Consumer<'a>: Sized {
    /// Asynchronously acquires a payload for reading data.
    ///
    /// This function will wait until data is available in the databus.
    async fn acquire_read(&'a self) -> ReadPayload<'a, Self>;

    /// Called by `ReadPayload` when it is dropped to release the buffer.
    ///
    /// This method is intended for internal use by the payload's RAII mechanism
    /// and should not be called directly by the user.
    fn release_read(&'a self, consumed_bytes: usize);

    // fn is_transformer_configured(&self) -> bool {
    //     false
    // }

    /// Helper to create an `InPort` for this `Consumer`.
    fn in_port(&'a self) -> InPort<'a, Dmy, Self> {
        InPort::Consumer(self)
    }
}

/// A trait for components to which audio data can be written asynchronously.
#[allow(async_fn_in_trait)]
pub trait Producer<'a>: Sized {
    /// Asynchronously acquires a payload for writing data.
    ///
    /// This function will wait until space is available in the databus.
    async fn acquire_write(&'a self) -> WritePayload<'a, Self>;

    /// Called by `WritePayload` when it is dropped to commit the written data.
    ///
    /// This method is intended for internal use by the payload's RAII mechanism
    /// and should not be called directly by the user.
    fn release_write(&'a self, buf: &'a mut [u8], metadata: Metadata);

    /// Helper to create an `OutPort` for this `Producer`.
    fn out_port(&'a self) -> OutPort<'a, Dmy, Self> {
        OutPort::Producer(self)
    }
}

/// A trait for components that support in-place modification of a buffer.
#[allow(async_fn_in_trait)]
pub trait Transformer<'a>: Sized {
    /// Asynchronously acquires a payload for in-place transformation.
    ///
    /// This operation will wait until the databus contains data that is ready
    /// to be processed.
    async fn acquire_transform(&'a self) -> TransformPayload<'a, Self>;

    /// Called by `TransformPayload` when it is dropped to finalize the transformation.
    ///
    /// This method is intended for internal use by the payload's RAII mechanism
    /// and should not be called directly by the user.
    fn release_transform(&'a self, buf: &'a mut [u8], metadata: Metadata);

    /// Helper to create an `InPlacePort` for this `Transformer`.
    fn inplace_port(&'a self) -> InPlacePort<'a, Self> {
        InPlacePort::Transformer(self)
    }
}