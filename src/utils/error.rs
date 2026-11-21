use crate::core::generations::GenerationError;
use crate::core::package::PackageError;
use crate::core::repo::RepositoryError;
use crate::core::resolver::ResolverError;
use crate::core::store::StoreError;
use crate::core::transaction::TransactionError;
use crate::fetch::FetchError;
use crate::fetch::verify::VerifyError;
use std::fmt::Display;
use std::io;

#[derive(Debug)]
pub enum Error {
    InvalidArgs(String),
    Io(io::Error),
    Package(PackageError),
    Store(StoreError),
    Generation(GenerationError),
    Database(String),
    Repository(RepositoryError),
    Resolver(ResolverError),
    Fetch(FetchError),
    Verify(VerifyError),
    Configuration(String),
    Transaction(TransactionError),
    Custom(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InvalidArgs(msg) => write!(f, "Invalid argument: {msg}"),
            Error::Io(e) => write!(f, "I/O Error: {e}"),
            Error::Package(e) => write!(f, "Package Error: {e}"),
            Error::Store(e) => write!(f, "Store Error: {e}"),
            Error::Generation(e) => write!(f, "Generation Error: {e}"),
            Error::Database(e) => write!(f, "Database Error: {e}"),
            Error::Repository(e) => write!(f, "Repository Error: {e}"),
            Error::Resolver(e) => write!(f, "Resolver Error: {e}"),
            Error::Fetch(e) => write!(f, "Fetching error: {e}"),
            Error::Verify(e) => write!(f, "Verify error: {e}"),
            Error::Configuration(e) => write!(f, "Configuration error: {e}"),
            Error::Transaction(e) => write!(f, "Transaction error: {e}"),
            Error::Custom(e) => write!(f, "Error: {e}"),
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
