#[derive(Debug)]
pub enum Error {
    Encoder(crate::encoder::Error),
}

impl From<crate::encoder::Error> for Error {
    fn from(err: crate::encoder::Error) -> Self {
        Error::Encoder(err)
    }
}

impl embedded_io::Error for Error {
    fn kind(&self) -> embedded_io::ErrorKind {
        match self {
            Error::Encoder(err) => err.kind(),
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Encoder(err) => err.fmt(f),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}