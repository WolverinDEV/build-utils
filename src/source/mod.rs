use std::path::PathBuf;

mod file;
pub use file::*;

pub trait BuildSource {
    fn name(&self) -> &str;
    fn local_directory(&self) -> &PathBuf;
}