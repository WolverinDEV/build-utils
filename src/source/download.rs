use crate::source::{BuildSource};
use std::path::{PathBuf};
use std::process::Command;
use std::io::ErrorKind;
use lazy_static::lazy_static;
use std::ops::Deref;
use crate::util::{create_temporary_path, TemporaryPath, execute_build_command};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use crate::build::BuildStepError;

#[derive(Ord, PartialOrd, Eq, PartialEq, Debug)]
enum GitBinaryStatus {
    /// Ok, version is the first argument
    Ok(String),
    NotFound,
    Outdated(String),
    Unknown(String)
}

fn check_git() -> GitBinaryStatus {
    match Command::new("git")
                .arg("--version")
                .output() {
        Ok(result) => {
            let version = String::from_utf8(result.stdout).expect("command result isn't utf-8")
                .lines().nth(0).map(|e| e.to_owned());
            if let Some(version) = version {
                if version.contains(" 2.") {
                    GitBinaryStatus::Ok(version)
                } else {
                    GitBinaryStatus::Ok(version)
                    //GitBinaryStatus::Outdated(version)
                }
            } else {
                GitBinaryStatus::Unknown(format!("truncated git version output"))
            }
        },
        Err(error) => {
            if error.kind() == ErrorKind::NotFound {
                GitBinaryStatus::NotFound
            } else {
                GitBinaryStatus::Unknown(format!("{:?}", error).to_owned())
            }
        }
    }
}

lazy_static! {
    static ref GIT_STATUS: GitBinaryStatus = check_git();
}

pub struct BuildSourceGit {
    repository_url: String,
    /* TODO: Branch? */
    revision: Option<String>,

    checkout_submodule: bool,
    skip_revision_checkout: bool,

    checkout_folder: Option<PathBuf>,
    local_folder: Option<TemporaryPath>
}

impl BuildSourceGit {
    pub fn builder(repository_url: String) -> BuildSourceGitBuilder {
        BuildSourceGitBuilder::new(repository_url)
    }

    fn temporary_directory_name(&self) -> String {
        let mut hash = DefaultHasher::new();
        self.repository_url.hash(&mut hash);
        self.revision.as_ref().map(|e| e.hash(&mut hash));
        let hash = hash.finish();
        let hash = base64::encode(hash.to_be_bytes()).replace("/", "_");

        let project_name = self.repository_url.split("/").last().unwrap_or("__unknown");
        format!("git_{}_{}", project_name, hash).to_owned()
    }
}

impl BuildSource for BuildSourceGit {
    fn name(&self) -> &str {
        "remote git repository"
    }

    fn hash(&self, target: &mut Box<dyn Hasher>) {
        self.repository_url.hash(target);
        self.revision.hash(target);
    }

    fn setup(&mut self) -> Result<(), BuildStepError> {
        if self.local_folder.is_some() {
            return Err(BuildStepError::new_simple("the source has already been initialized"));
        }

        if !matches!(GIT_STATUS.deref(), GitBinaryStatus::Ok(_)) {
            return Err(BuildStepError::new_simple(format!("git error: {:?}", GIT_STATUS.deref())));
        }

        let target_folder = match create_temporary_path(&self.temporary_directory_name(), self.checkout_folder.as_ref()) {
            Ok(folder) => {
                folder.release(); /* FIXME! */
                self.local_folder = Some(folder.clone());
                folder
            },
            Err(err) => return Err(BuildStepError::new_simple(format!("failed to create git checkout directory: {:?}", err)))
        };

        let mut repository_exists = false;
        if target_folder.join(".git").exists() {
            println!("Updating existing repository ({:?})", target_folder);

            let mut command = Command::new("git");
            command.arg("fetch")
                   .current_dir(target_folder.deref());

            if let Err(error) = execute_build_command(&mut command, "git fetch failed") {
                if error.stderr().find("not a git repository").is_none() {
                    return Err(error);
                } else {
                    std::fs::remove_dir_all(target_folder.deref())
                        .map_err(|err| BuildStepError::new_io("failed to remove old temporary checkout directory", err))?;

                    std::fs::create_dir_all(target_folder.deref())
                        .map_err(|err| BuildStepError::new_io("failed to create new temporary checkout directory", err))?;
                }
            } else {
                repository_exists = true;
            }
        }

        if !repository_exists {
            println!("Cloning git repository");

            let mut command = Command::new("git");
            command.arg("clone")
                   .arg(&self.repository_url)
                   .arg(target_folder.deref());

            execute_build_command(&mut command, "git clone failed")?;
        }

        if !self.skip_revision_checkout {
            let revision = self.revision.clone().unwrap_or("HEAD".to_owned());
            println!("Checking out revision {}", &revision);


            let mut command = Command::new("git");
            command.arg("reset")
                   .arg("--hard")
                   .arg(&revision)
                   .current_dir(target_folder.deref());

            execute_build_command(&mut command, "git revision checkout failed")?;
        }

        Ok(())
    }

    fn local_directory(&self) -> &PathBuf {
        self.local_folder.as_ref().expect("expected a path")
            .path()
    }

    fn cleanup(&mut self) {
        /* FIXME: Remove this? */
        self.local_folder.as_mut().map(|e| e.release());
        self.local_folder = None;
    }
}

pub struct BuildSourceGitBuilder {
    inner: BuildSourceGit
}

impl BuildSourceGitBuilder {
    fn new(repository_url: String) -> Self {
        BuildSourceGitBuilder {
            inner: BuildSourceGit {
                repository_url,

                checkout_submodule: false,
                skip_revision_checkout: false,

                checkout_folder: None,
                local_folder: None,
                revision: None
            }
        }
    }

    pub fn checkout_submodule(mut self, enabled: bool) -> Self {
        self.inner.checkout_submodule = enabled;
        self
    }

    pub fn checkout_folder(mut self, path: Option<PathBuf>) -> Self {
        self.inner.checkout_folder = path;
        self
    }

    pub fn revision(mut self, revision: Option<String>) -> Self {
        self.inner.revision = revision;
        self
    }

    pub fn skip_revision_checkout(mut self, enabled: bool) -> Self {
        self.inner.skip_revision_checkout = enabled;
        self
    }

    pub fn build(self) -> BuildSourceGit {
        self.inner
    }
}

#[cfg(test)]
mod test {
    use crate::source::{BuildSourceGit, BuildSource};

    #[test]
    fn test_git() {
        let mut source = BuildSourceGit::builder("https://github.com/WolverinDEV/libnice.git".to_owned())
            .build();

        source.setup().unwrap();
    }
}