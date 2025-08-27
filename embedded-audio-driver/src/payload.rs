use core::ops::{Deref, DerefMut};

macro_rules! impl_deref_valid_length {
    ($struct_name:ident, <'a, $g:ident: $trait:path>) => {
        impl<'a, $g: $trait> Deref for $struct_name<'a, $g> {
            type Target = [u8];
            /// Dereferences to a slice containing only the valid data.
            fn deref(&self) -> &Self::Target {
                &self.data[..self.metadata.valid_length]
            }
        }
    };
}

macro_rules! impl_deref_full_data {
    ($struct_name:ident, <'a, $g:ident: $trait:path>) => {
        impl<'a, $g: $trait> Deref for $struct_name<'a, $g> {
            type Target = [u8];
            fn deref(&self) -> &Self::Target {
                self.data
            }
        }
    };
}

macro_rules! impl_deref_mut_full_data {
    ($struct_name:ident, <'a, $g:ident: $trait:path>) => {
        impl<'a, $g: $trait> DerefMut for $struct_name<'a, $g> {
            fn deref_mut(&mut self) -> &mut <Self as Deref>::Target {
                self.data
            }
        }
    };
}

use crate::databus::{Consumer, Producer, Transformer};

/// Metadata for payload data, including position and length information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Metadata {
    /// Position of this payload in a sequence.
    pub position: Position,
    /// Length of valid data in the payload buffer, in bytes.
    pub valid_length: usize,
}

impl Metadata {
    pub fn new(position: Position, valid_length: usize) -> Self {
        Self {
            position,
            valid_length,
        }
    }
}

/// Position of a payload within a data sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum Position {
    /// A single, complete payload (not part of a sequence).
    Single,
    /// The first payload in a sequence.
    First,
    /// The last payload in a sequence.
    Last,
    /// A middle payload in a sequence.
    #[default]
    Middle,
}

// --- Read Payload ---

/// A RAII guard representing a readable buffer acquired from a `Consumer`.
///
/// When this guard is dropped, the buffer is automatically released back to the `Consumer`.
/// This payload provides immutable access to the data.
pub struct ReadPayload<'a, C: Consumer<'a>> {
    data: &'a [u8],
    pub metadata: Metadata,
    consumer: &'a C,
}

impl<'a, C: Consumer<'a>> ReadPayload<'a, C> {
    pub fn new(data: &'a [u8], metadata: Metadata, consumer: &'a C) -> Self {
        Self { data, metadata, consumer }
    }
}

impl_deref_valid_length!(ReadPayload, <'a, C: Consumer<'a>>);

impl<'a, C: Consumer<'a>> Drop for ReadPayload<'a, C> {
    fn drop(&mut self) {
        self.consumer.release_read(self.metadata.valid_length);
    }
}

// --- Write Payload ---

/// A RAII guard representing a writable buffer acquired from a `Producer`.
///
/// When this guard is dropped, the written data and its metadata are "committed"
/// back to the `Producer`.
pub struct WritePayload<'a, P: Producer<'a>> {
    data: &'a mut [u8],
    metadata: Metadata, // Kept private to enforce setter usage.
    producer: &'a P,
}

impl<'a, P: Producer<'a>> WritePayload<'a, P> {
    pub fn new(data: &'a mut [u8], producer: &'a P) -> Self {
        Self {
            data,
            metadata: Metadata::default(),
            producer,
        }
    }

    /// Sets the length of the valid data written to the payload.
    pub fn set_valid_length(&mut self, length: usize) {
        self.metadata.valid_length = length.min(self.data.len());
    }

    /// Sets the position of this payload within a sequence.
    pub fn set_position(&mut self, position: Position) {
        self.metadata.position = position;
    }
}

impl_deref_full_data!(WritePayload, <'a, P: Producer<'a>>);
impl_deref_mut_full_data!(WritePayload, <'a, P: Producer<'a>>);

impl<'a, P: Producer<'a>> Drop for WritePayload<'a, P> {
    fn drop(&mut self) {
        // Replace the buffer with a dummy slice to transfer ownership back.
        let dummy_slice = &mut [];
        let buffer = core::mem::replace(&mut self.data, dummy_slice);
        self.producer.release_write(buffer, self.metadata);
    }
}

// --- Transform Payload ---

/// A RAII guard representing a readable and writable buffer for in-place operations.
///
/// Acquired from a `Transformer`. When this guard is dropped, the transformation is
/// considered complete, making the buffer available for the next consumer or transformer.
pub struct TransformPayload<'a, T: Transformer<'a>> {
    data: &'a mut [u8],
    /// Metadata from the previous stage, which can be read by the transformer.
    pub metadata: Metadata,
    transformer: &'a T,
}

impl<'a, T: Transformer<'a>> TransformPayload<'a, T> {
    pub fn new(data: &'a mut [u8], metadata: Metadata, transformer: &'a T) -> Self {
        Self { data, metadata, transformer }
    }

    /// Allows the transformer to update the valid length if the transformation
    /// changes the amount of data (e.g., compression).
    pub fn set_valid_length(&mut self, length: usize) {
        self.metadata.valid_length = length.min(self.data.len());
    }
}

impl_deref_valid_length!(TransformPayload, <'a, T: Transformer<'a>>);
impl_deref_mut_full_data!(TransformPayload, <'a, T: Transformer<'a>>);

impl<'a, T: Transformer<'a>> Drop for TransformPayload<'a, T> {
    fn drop(&mut self) {
        let dummy_slice = &mut [];
        let buffer = core::mem::replace(&mut self.data, dummy_slice);
        self.transformer.release_transform(buffer, self.metadata);
    }
}
