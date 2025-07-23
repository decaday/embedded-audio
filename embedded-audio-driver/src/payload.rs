use crate::databus::Release;

/// Metadata for payload data, including position and length information
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metadata {
    /// Position of this payload in a sequence
    pub position: Position,
    /// Length of valid data in the payload buffer
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

/// Position of payload in a data sequence
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Position {
    /// Single complete payload (not part of a sequence)
    Single,
    /// First payload in a sequence
    First,
    /// Last payload in a sequence
    Last,
    /// Middle payload in a sequence
    Middle,
}

impl From<Position> for u8 {
    fn from(position: Position) -> Self {
        position as u8
    }
}

impl TryFrom<u8> for Position {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            x if x == Position::Single as u8 => Ok(Position::Single),
            x if x == Position::First as u8 => Ok(Position::First),
            x if x == Position::Last as u8 => Ok(Position::Last),
            x if x == Position::Middle as u8 => Ok(Position::Middle),
            _ => Err(()),
        }
    }
}

/// A generic payload that can be backed by different databus implementations
pub struct Payload<'a, T: Release<'a>> {
    /// Mutable reference to the data buffer
    pub data: &'a mut [u8],
    /// Metadata about this payload
    pub metadata: Metadata,
    pub is_write: bool,
    pub release: &'a T,
}

impl<'a, T: Release<'a>> Payload<'a, T> {
    /// Creates a new payload with the given data, metadata, and completion callback
    pub fn new(
        data: &'a mut [u8],
        metadata: Metadata,
        is_write: bool,
        release: &'a T,
    ) -> Self {
        Self {
            data,
            metadata,
            is_write,
            release,
        }
    }

    /// Updates the valid length in the metadata
    pub fn set_valid_length(&mut self, length: usize) {
        self.metadata.valid_length = length.min(self.data.len());
    }

    /// Updates the position in the metadata
    pub fn set_position(&mut self, position: Position) {
        self.metadata.position = position;
    }
}

impl<'a, T: Release<'a>> core::ops::Deref for Payload<'a, T> {
    type Target = [u8];
    
    fn deref(&self) -> &Self::Target {
        self.data
    }
}

impl<'a, T: Release<'a>> core::ops::DerefMut for Payload<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data
    }
}

impl<'a, T: Release<'a>> Drop for Payload<'a, T> {
    fn drop(&mut self) {
        // Return the buffer to the slot.
        let dummy_slice = &mut [];
        let buffer = core::mem::replace(&mut self.data, dummy_slice);

        // Execute the completion callback when the payload is dropped
        self.release.release(buffer, self.metadata, self.is_write);
    }
}