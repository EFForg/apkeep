use std::error::Error as StdError;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    Dummy,
    Other(Box<dyn StdError>),
}

impl From<Box<dyn StdError>> for Error {
    fn from(err: Box<dyn StdError>) -> Error {
        Self::Other(err)
    }
}

impl StdError for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Dummy => write!(f, "Dummy"),
            Self::Other(err) => err.fmt(f),
        }
    }
}
