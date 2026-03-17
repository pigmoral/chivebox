use std::fmt;

#[derive(Debug)]
pub enum Error {
    UnknownTarget(String),
    IoError(std::io::Error),
    BuildFailure(String),
    AppletListFailed(String),
    MissingToolchain(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnknownTarget(s) => write!(f, "Unknown target: {}", s),
            Error::IoError(e) => write!(f, "IO error: {}", e),
            Error::BuildFailure(s) => write!(f, "Build failed: {}", s),
            Error::AppletListFailed(s) => write!(f, "Failed to get applet list: {}", s),
            Error::MissingToolchain(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::IoError(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
