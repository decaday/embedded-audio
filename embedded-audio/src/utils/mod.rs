/// Trait for types that can be converted from a fixed-size byte array.
/// Each implementing type specifies its own byte size through SIZE constant.
pub trait FromBytes<const SIZE: usize>: Sized {
    /// Convert from bytes in little-endian format
    fn from_le_bytes(bytes: [u8; SIZE]) -> Self;
}

impl FromBytes<1> for i8 {
    fn from_le_bytes(bytes: [u8; 1]) -> Self {
        i8::from_le_bytes(bytes)
    }
}

impl FromBytes<2> for i16 {
    fn from_le_bytes(bytes: [u8; 2]) -> Self {
        i16::from_le_bytes(bytes)
    }
}

impl FromBytes<4> for i32 {
    fn from_le_bytes(bytes: [u8; 4]) -> Self {
        i32::from_le_bytes(bytes)
    }
}

impl FromBytes<8> for i64 {
    fn from_le_bytes(bytes: [u8; 8]) -> Self {
        i64::from_le_bytes(bytes)
    }
}

impl FromBytes<1> for u8 {
    fn from_le_bytes(bytes: [u8; 1]) -> Self {
        bytes[0]
    }
}

impl FromBytes<2> for u16 {
    fn from_le_bytes(bytes: [u8; 2]) -> Self {
        u16::from_le_bytes(bytes)
    }
}

impl FromBytes<4> for u32 {
    fn from_le_bytes(bytes: [u8; 4]) -> Self {
        u32::from_le_bytes(bytes)
    }
}

impl FromBytes<8> for u64 {
    fn from_le_bytes(bytes: [u8; 8]) -> Self {
        u64::from_le_bytes(bytes)
    }
}

impl FromBytes<4> for f32 {
    fn from_le_bytes(bytes: [u8; 4]) -> Self {
        f32::from_le_bytes(bytes)
    }
}

impl FromBytes<8> for f64 {
    fn from_le_bytes(bytes: [u8; 8]) -> Self {
        f64::from_le_bytes(bytes)
    }
}


#[cfg(test)]
mod tests {
    #[test]
    fn test_i8_conversion() {
        let value: i8 = 123;
        let bytes = value.to_le_bytes();
        assert_eq!(i8::from_le_bytes(bytes), value);
    }

    #[test]
    fn test_i16_conversion() {
        let value: i16 = 12345;
        let bytes = value.to_le_bytes();
        assert_eq!(i16::from_le_bytes(bytes), value);
    }

    #[test]
    fn test_i32_conversion() {
        let value: i32 = -1234567;
        let bytes = value.to_le_bytes();
        assert_eq!(i32::from_le_bytes(bytes), value);
    }

    #[test]
    fn test_f32_conversion() {
        let value: f32 = 123.456;
        let bytes = value.to_le_bytes();
        assert_eq!(f32::from_le_bytes(bytes), value);
    }

    #[test]
    fn test_f64_conversion() {
        let value: f64 = -987.654321;
        let bytes = value.to_le_bytes();
        assert_eq!(f64::from_le_bytes(bytes), value);
    }
}