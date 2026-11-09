use std::fmt::{Display};
use std::io;

#[derive(Debug)]
pub enum Error {
    InvalidArgs(String),
    Io(io::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InvalidArgs(msg) => writeln!(f, "Invalid argument: {msg}"),
            Error::Io(e) => writeln!(f, "I/O Error: {e}")
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;