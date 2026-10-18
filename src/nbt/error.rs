use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    InvalidGzip,
    Truncated,
    UnknownType(u8),
    InvalidUtf8,
    InvalidCount,
    TooDeep,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidGzip => write!(f, "invalid gzip data"),
            Error::Truncated => write!(f, "truncated data"),
            Error::UnknownType(id) => write!(f, "unknown tag type {id}"),
            Error::InvalidUtf8 => write!(f, "invalid utf-8 string"),
            Error::InvalidCount => write!(f, "invalid element count"),
            Error::TooDeep => write!(f, "tag nesting too deep"),
        }
    }
}

impl std::error::Error for Error {}
