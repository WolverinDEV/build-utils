use std::path::PathBuf;

mod file;
pub use file::*;

mod download;
pub use download::*;
use crate::build::BuildStepError;
use std::hash::Hasher;

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum SourceSetupError {
    SourceDoesNotExists,
    SourceIsNotReadable,
    NoSpaceOnDisk,
    TemporaryPathNotWriteable,
    AlreadyInitialized,
    Unknown(String)
}

pub trait BuildSource {
    /// The name of the source
    fn name(&self) -> &str;

    /// Generate a unique hash which identifies the source and possible changes
    fn hash(&self, target: &mut Box<dyn Hasher>);

    fn setup(&mut self) -> Result<(), BuildStepError>;
    fn local_directory(&self) -> &PathBuf;
    fn cleanup(&mut self);
}