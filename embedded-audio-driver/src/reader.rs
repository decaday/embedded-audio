#[derive(Debug)]
pub enum Error {
    Decoder(crate::decoder::Error),
}

impl From<crate::decoder::Error> for Error {
    fn from(err: crate::decoder::Error) -> Self {
        Error::Decoder(err)
    }
}

impl embedded_io::Error for Error {
    fn kind(&self) -> embedded_io::ErrorKind {
        match self {
            Error::Decoder(err) => err.kind(),
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Decoder(err) => err.fmt(f),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}