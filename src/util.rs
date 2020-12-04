use std::env;
use crate::build::{LibraryType, BuildStepError};
use std::path::PathBuf;
use std::sync::Arc;
use std::ops::Deref;
use std::fmt::{Debug, Formatter};
use std::process::{Command};

/*
fn resolve_environment_variable_(build_name: &str, key_name: &str, key_general: &str) -> Option<String> {
    let name = format!(build_name, build_name);
    if let Ok(value) = env::var(name) {
        Some(value)
    } else if let Ok(value) = env::var(key_general) {
        Some(value)
    } else {
        None
    }
}
*/

pub macro_rules! resolve_env_var {
    ($build_name:ident, $key:expr) => {{
        let full_name = format!("rbuild_{}_{}", $build_name, $key);
        let fallback_name = format!("rbuild_{}", $key);
        if let Ok(value) = env::var(full_name) {
            Some(value)
        } else if let Ok(value) = env::var(fallback_name) {
            Some(value)
        } else {
            None
        }
    }};
}

pub enum BuildLibraryTypeError {
    NotPresent,
    InvalidValue(String)
}

fn parse_build_library_type(library_type: &str) -> Option<LibraryType> {
    let library_type = library_type.to_lowercase();
    match library_type.as_ref() {
        "static" => Some(LibraryType::Static),
        "shared" => Some(LibraryType::Shared),
        _ => None
    }
}

pub fn build_library_type(build_name: &str) -> Result<LibraryType, BuildLibraryTypeError> {
    if let Some(value) = resolve_env_var!(build_name, "library_type") {
        parse_build_library_type(&value)
            .ok_or(BuildLibraryTypeError::InvalidValue(value))
    } else {
        Err(BuildLibraryTypeError::NotPresent)
    }
}

pub fn install_prefix(build_name: &str) -> Option<PathBuf> {
    if let Some(path) = resolve_env_var!(build_name, "install_prefix")  {
        Some(PathBuf::from(path))
    } else if let Ok(path) = env::var("OUT_DIR") {
        /* we're doing cargo right now */
        Some(PathBuf::from(path))
    } else {
        None
    }
}

struct TemporaryPathInner {
    path: PathBuf,
    released: bool
}

impl Drop for TemporaryPathInner {
    fn drop(&mut self) {
        if !self.released {
            if let Err(error) = std::fs::remove_dir_all(&self.path) {
                eprintln!("Failed to remote temporary directory: {:?}", error);
            }
        }
    }
}

#[derive(Clone)]
pub struct TemporaryPath {
    inner: Arc<TemporaryPathInner>
}

impl TemporaryPath {
    pub fn from_persistent(path: PathBuf) -> Self {
        TemporaryPath{
            inner: Arc::new(TemporaryPathInner{ path, released: true })
        }
    }

    pub fn path(&self) -> &PathBuf {
        &self.inner.path
    }

    pub fn release(&self) -> &Self {
        let mut_released = unsafe { &mut *(&self.inner.released as *const bool as *mut bool) };
        *mut_released = true;

        self
    }
}

impl Deref for TemporaryPath {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        self.path()
    }
}

impl Debug for TemporaryPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.inner.path.fmt(f)
    }
}

pub fn create_temporary_path(folder_name: &str, base_dir: Option<&PathBuf>) -> std::io::Result<TemporaryPath> {
    let path = if let Some(base_dir) = base_dir {
        base_dir.join(folder_name)
    } else if let Ok(path) = env::var("OUT_DIR") {
        /* Seems like a cargo build. Use that directory as temp so we don't junk the system temp directory */
        PathBuf::from(path).join(folder_name)
    } else {
        env::temp_dir().join(folder_name)
    };

    std::fs::create_dir_all(&path).map(|_| TemporaryPath{ inner: Arc::new(TemporaryPathInner{ path, released: false })})
}

fn verbose_commands_enabled() -> bool {
    /* TODO: Some kind of env variable */
    true
}

pub fn execute_build_command(command: &mut Command, error_detail: &str) -> Result<(String, String), BuildStepError> {
    let output = command.output()
        .map_err(|err| BuildStepError::new_io(error_detail, err))?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    if verbose_commands_enabled() {
        let error_code = if let Some(code) = output.status.code() { format!("{}", code) } else { "no error code".to_owned() };
        println!("> {:?} -> {}", command, error_code);
        if !stdout.is_empty() {
            println!("----------------- Stdout -----------------");
            println!("{}", &stdout);
        }

        if !stderr.is_empty() {
            println!("----------------- Stderr -----------------");
            println!("{}", &stderr);
        }
    }

    if !output.status.success() {
        return Err(BuildStepError::new(error_detail.to_owned(), stdout, stderr));
    }

    Ok((stdout, stderr))
}