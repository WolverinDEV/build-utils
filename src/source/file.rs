use crate::source::{BuildSource};
use std::path::PathBuf;
use crate::build::BuildStepError;
use std::hash::{Hasher, Hash};

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Copy, Clone)]
pub enum BuildSourceDirectoryError {
    TargetDoesNotExists,
    TargetIsNotADirectory,
    DirectoryNotAccessable
}

pub struct BuildSourceDirectory {
    path: PathBuf
}

impl BuildSourceDirectory {
    pub fn new(target: PathBuf) -> Result<Self, BuildSourceDirectoryError> {
        if target.exists() {
            Err(BuildSourceDirectoryError::TargetDoesNotExists)
        } else if !target.is_dir() {
            Err(BuildSourceDirectoryError::TargetIsNotADirectory)
        } else if let Ok(_) = target.read_dir() {
            Err(BuildSourceDirectoryError::DirectoryNotAccessable)
        } else {
            Ok(BuildSourceDirectory{ path: target })
        }
    }
}

impl BuildSource for BuildSourceDirectory {
    fn name(&self) -> &str {
        "directory"
    }

    fn hash(&self, target: &mut Box<dyn Hasher>) {
        self.path.hash(target);
    }

    fn setup(&mut self) -> Result<(), BuildStepError> {
        Ok(())
    }

    fn local_directory(&self) -> &PathBuf {
        &self.path
    }

    fn cleanup(&mut self) { }
}