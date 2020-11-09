use crate::source::BuildSource;
use std::ops::{Deref};
use std::path::PathBuf;

mod meson;
pub use meson::*;
use crate::util::{TemporaryPath, create_temporary_path, install_prefix, build_library_type, BuildLibraryTypeError};
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub enum LibraryType {
    Static,
    Shared
}

impl ToString for LibraryType {
    fn to_string(&self) -> String {
        match self {
            LibraryType::Shared => "dylib",
            LibraryType::Static => "static"
        }.to_owned()
    }
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Debug)]
pub enum LinkSearchKind {
    Dependency,
    Crate,
    Native,
    Framework,
    All
}

impl ToString for LinkSearchKind {
    fn to_string(&self) -> String {
        match self {
            LinkSearchKind::Dependency => "dependency",
            LinkSearchKind::Crate => "crate",
            LinkSearchKind::Native => "native",
            LinkSearchKind::Framework => "framework",
            LinkSearchKind::All => "all"
        }.to_owned()
    }
}

#[derive(Debug)]
pub enum BuildCreateError {
    Unknown,
    MissingName,
    MissingSource,
    Missing(String),
    FailedToCreateBuildDirectory(std::io::Error),
    InvalidEnvLibraryType(String),
}

/*
    struct MesonBuildOptions {}
    struct MesonBuild {
        options: BuildOptions<MesonBuildOptions>
    }
    impl BuildConstructor for MesonBuild {}
    impl Build for MesonBuild {}

    let build: MesonBuild = BuildBuilder::<MesonBuild>::new()
        .name("libnice")
        .source(box BuildSourceDirectory::new("./libnice").expect("failed to create build source"))
        .apply_special_options(|options| {})
        .build();
 */

#[derive(Debug)]
pub struct BuildError {
    step: String,
    error: BuildStepError
}

impl BuildError {
    pub fn pretty_format(&self) -> String {
        let mut result = String::with_capacity(self.error.stdout.len() + self.error.stderr.len() + self.step.len() + self.error.detail.len() + 200);

        result.push_str(format!("Build step \"{}\" errored: {}\n", &self.step, &self.error.detail).as_ref());
        if !self.error.stdout.is_empty() {
            result.push_str("----------------- Stdout -----------------\n");
            result.push_str(&self.error.stdout);
        }

        if !self.error.stderr.is_empty() {
            result.push_str("----------------- Stderr -----------------\n");
            result.push_str(&self.error.stderr);
        }

        result
    }
}

#[derive(Debug)]
pub struct BuildStepError {
    detail: String,

    stdout: String,
    stderr: String
}

impl BuildStepError {
    pub fn new_simple<S>(detail: S) -> Self
        where S: Into<String>
    {
        Self::new(detail.into(), String::new(), String::new())
    }

    pub fn new_io<S>(detail: S, error: std::io::Error) -> Self
        where S: Into<String>
    {
        Self::new(detail.into(), String::new(), format!("IOError: {}", error.to_string()))
    }

    pub fn new(detail: String, stdout: String, stderr: String) -> Self {
        BuildStepError{
            detail,

            stdout,
            stderr
        }
    }

    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    pub fn stderr(&self) -> &str {
        &self.stderr
    }
}

pub struct BuildLibrary {
    name: String,
    kind: Option<LibraryType>
}

impl ToString for BuildLibrary {
    fn to_string(&self) -> String {
        if let Some(kind) = &self.kind {
            format!("{}={}", kind.to_string(), self.name).to_owned()
        } else {
            self.name.clone()
        }
    }
}

pub struct BuildLibraryPath {
    path: PathBuf,
    kind: LinkSearchKind
}

impl ToString for BuildLibraryPath {
    fn to_string(&self) -> String {
        if self.kind == LinkSearchKind::All {
            self.path.to_string_lossy().into_owned()
        } else {
            format!("{}={}", self.kind.to_string(), self.path.to_string_lossy().into_owned()).to_owned()
        }
    }
}

pub struct BuildResult {
    libraries: Vec<BuildLibrary>,
    library_paths: Vec<BuildLibraryPath>,
    custom_compiler_emits: Vec<String>
}

impl BuildResult {
    pub fn new() -> Self {
        BuildResult{
            libraries: Vec::new(),
            library_paths: Vec::new(),
            custom_compiler_emits: Vec::new()
        }
    }

    pub fn add_library(&mut self, name: String, kind: Option<LibraryType>) -> &mut Self {
        self.libraries.push(BuildLibrary{ name, kind });
        self
    }

    pub fn libraries(&self) -> &Vec<BuildLibrary> {
        &self.libraries
    }

    pub fn add_library_path(&mut self, path: PathBuf, kind: Option<LinkSearchKind>) -> &mut Self {
        /* FIXME: Remove duplicated paths */
        self.library_paths.push(BuildLibraryPath{ path, kind: kind.unwrap_or(LinkSearchKind::All) });
        self
    }

    pub fn library_paths(&self) -> &Vec<BuildLibraryPath> {
        &self.library_paths
    }

    pub fn add_emit(&mut self, line: String) -> &mut Self {
        self.custom_compiler_emits.push(line);
        self
    }

    pub fn emit_cargo(&self) {
        self.library_paths.iter().for_each(|path| {
            println!("cargo:rustc-link-search={}", path.to_string());
        });

        self.libraries.iter().for_each(|path| {
            println!("cargo:rustc-link-search={}", path.to_string());
        });

        self.custom_compiler_emits.iter().for_each(|emit| {
            println!("cargo:{}", emit);
        });
    }
}

pub trait BuildStep {
    fn name(&self) -> &str;

    /// Generate a hash which uniquely identifies the build options
    fn hash(&self, hasher: &mut Box<dyn Hasher>);

    /* some generic function */
    fn execute(&mut self, build: &Build, result: &mut BuildResult) -> Result<(), BuildStepError>;
}

pub struct Build {
    name: String,
    source: Box<dyn BuildSource>,
    build_hash: u64,

    steps: Vec<RefCell<Box<dyn BuildStep>>>,

    library_type: LibraryType,

    build_path: TemporaryPath,
    install_prefix: Option<PathBuf>,
}

impl Build {
    /// Create a new build builder
    pub fn builder() -> BuildBuilder {
        BuildBuilder::new()
    }

    /// Get the name of the target build
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The target library tyoe which should be build
    pub fn library_type(&self) -> LibraryType {
        self.library_type
    }

    /// Get the target install prefix where the library should be installed into.
    /// This might be unset.
    pub fn install_prefix(&self) -> &Option<PathBuf> {
        &self.install_prefix
    }

    /// Get the temporary build path where you should build into
    pub fn build_path(&self) -> &PathBuf {
        &self.build_path.deref()
    }

    pub fn source(&self) -> &Box<dyn BuildSource> {
        &self.source
    }

    pub fn build_hash(&self) -> u64 {
        self.build_hash
    }

    /// Execute the build and all its steps
    pub fn execute(&mut self) -> Result<BuildResult, BuildError> {
        if let Err(error) = self.source.setup() {
            return Err(BuildError{
                step: "source setup".to_owned(),
                error
            });
        }

        let mut result = BuildResult::new();
        for step in self.steps.iter() {
            let mut step = RefCell::borrow_mut(step);

            if let Err(err) = step.execute(self, &mut result) {
                return Err(BuildError{
                    step: step.name().to_owned(),
                    error: err
                })
            }
        }
        Ok(result)
    }
}

pub struct BuildBuilder {
    name: Option<String>,
    source: Option<Box<dyn BuildSource>>,

    steps: Vec<RefCell<Box<dyn BuildStep>>>,
    library_type: Option<LibraryType>,

    install_prefix: Option<PathBuf>,
    build_path: Option<PathBuf>,

    /* TODO: Make this variable environment editable */
    remove_build_dir: bool,
    /* TODO: Env */
}

impl BuildBuilder {
    fn new() -> Self {
        BuildBuilder {
            name: None,
            source: None,

            steps: Vec::new(),
            library_type: None,

            install_prefix: None,
            build_path: None,

            remove_build_dir: true
        }
    }

    pub fn build(self) -> Result<Box<Build>, BuildCreateError> {
        let name = if let Some(name) = self.name { name } else {
            return Err(BuildCreateError::MissingName);
        };
        let source = if let Some(source) = self.source { source } else {
            return Err(BuildCreateError::MissingSource);
        };

        let install_prefix = if let Some(prefix) = self.install_prefix {
            Some(prefix)
        } else if let Some(prefix) = install_prefix(&name) {
            Some(prefix)
        } else {
            None
        };

        let library_type = if let Some(ltype) = self.library_type {
            ltype
        }  else {
            match build_library_type(&name) {
                Ok(ltype) => ltype,
                Err(BuildLibraryTypeError::InvalidValue(value)) => return Err(BuildCreateError::InvalidEnvLibraryType(value)),
                Err(BuildLibraryTypeError::NotPresent) => LibraryType::Shared
            }
        };

        let build_hash = {
            let mut hash: Box<dyn Hasher> = Box::new(DefaultHasher::new());
            name.hash(&mut hash);
            source.hash(&mut hash);
            install_prefix.hash(&mut hash);
            library_type.hash(&mut hash);
            self.steps.iter().enumerate().for_each(|(index, step)| {
                let step = RefCell::borrow(step);
                index.hash(&mut hash);
                step.name().hash(&mut hash);
                step.hash(&mut hash);
            });
            hash.finish()
        };

        let hash_str = base64::encode(build_hash.to_be_bytes()).replace("/", "_");
        let build_path = match create_temporary_path(format!("build_{}_{}", &name, hash_str).as_ref(), self.build_path) {
            Ok(path) => path,
            Err(err) => return Err(BuildCreateError::FailedToCreateBuildDirectory(err))
        };

        if !self.remove_build_dir {
            build_path.release();
        }

        Ok(Box::new(Build{
            name,
            source,
            build_hash,

            steps: self.steps,
            library_type,

            build_path,
            install_prefix
        }))
    }

    pub fn name<V>(mut self, value: V) -> Self
        where V: Into<String>
    {
        self.name = Some(value.into());
        self
    }

    pub fn source(mut self, source: Box<dyn BuildSource>) -> Self {
        self.source = Some(source);
        self
    }

    pub fn build_path(mut self, path: PathBuf) -> Self {
        self.build_path = Some(path);
        self
    }

    pub fn install_prefix(mut self, path: PathBuf) -> Self {
        self.install_prefix = Some(path);
        self
    }

    pub fn library_type(mut self, ltype: LibraryType) -> Self {
        self.library_type = Some(ltype);
        self
    }

    pub fn remove_build_dir(mut self, enabled: bool) -> Self {
        self.remove_build_dir = enabled;
        self
    }

    pub fn add_step(mut self, step: Box<dyn BuildStep>) -> Self {
        self.steps.push(RefCell::new(step));
        self
    }
}


#[cfg(test)]
mod test {
    use crate::BuildStep;
    use crate::build::{Build, BuildResult, BuildStepError};
    use crate::source::{BuildSource};
    use std::path::PathBuf;
    use std::hash::Hasher;

    struct DummyBuildStep { }
    impl BuildStep for DummyBuildStep {
        fn name(&self) -> &str {
            "dummy"
        }

        fn hash(&self, _state: &mut Box<dyn Hasher>) { }

        fn execute(&mut self, _build: &Build, _result: &mut BuildResult) -> Result<(), BuildStepError> {
            Ok(())
        }
    }

    struct DummyBuildSource {}
    impl BuildSource for DummyBuildSource {
        fn name(&self) -> &str {
            "dummy"
        }

        fn hash(&self, _state: &mut Box<dyn Hasher>) { }

        fn setup(&mut self) -> Result<(), BuildStepError> {
            Ok(())
        }

        fn local_directory(&self) -> &PathBuf {
            unimplemented!()
        }

        fn cleanup(&mut self) { }
    }

    #[test]
    fn test_builder() {
        let mut build = Build::builder()
            .name("test")
            .source(Box::new(DummyBuildSource{}))
            .add_step(Box::new(DummyBuildStep{}))
            .build().expect("failed to create dummy build");
        build.execute().expect("build should have succeeded");
    }
}