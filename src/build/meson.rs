use crate::BuildStep;
use crate::util::execute_build_command;
use std::process::Command;
use crate::build::{BuildResult, Build, BuildStepError, LibraryType, LinkSearchKind};
use std::collections::HashMap;
use std::path::PathBuf;
use std::hash::{Hasher, Hash};

pub struct MesonBuild {
    callback_promote: Option<Box<dyn Fn(&str) -> Vec<String>>>,
    meson_options: HashMap<String, String>
}

impl MesonBuild {
    pub fn builder() -> MesonBuildBuilder {
        MesonBuildBuilder::new()
    }
}

impl BuildStep for MesonBuild {
    fn name(&self) -> &str {
        "meson build"
    }

    fn hash(&self, hasher: &mut Box<dyn Hasher>) {
        self.meson_options.iter().for_each(|(key, value)| {
            key.hash(hasher);
            value.hash(hasher);
        });
    }

    fn execute(&mut self, build: &Build, result: &mut BuildResult) -> Result<(), BuildStepError> {
        let build_path = build.build_path().to_str().expect("invalid build path");
        let source_path = build.source().local_directory().to_str().expect("invalid source path");

        let mut execute_setup = true;
        /* setup */
        while execute_setup {
            execute_setup = false;

            let mut command = Command::new("meson");
            command.arg("setup");
            command.args(&["--prefix", build.install_prefix().to_str().expect("invalid install prefix")]);

            match build.library_type {
                LibraryType::Shared => command.arg("-Ddefault_library=shared"),
                LibraryType::Static => command.arg("-Ddefault_library=static"),
            };

            self.meson_options.iter().for_each(|(key, value)| {
                command.arg(format!("-D{}={}", key, value));
            });

            command.arg(&build_path);
            command.arg(&source_path);

            if let Err(error) = execute_build_command(&mut command, "failed to setup build") {
                if let Some(line) = error.stdout.lines().find(|line| line.find("meson wrap promote ").is_some()) {
                    let argument = line.split("meson wrap promote ").nth(1).expect("missing promote arguments");

                    if let Some(callback) = &self.callback_promote {
                        let promote: Vec<String> = callback(argument);
                        if !promote.is_empty() {
                            for file in promote.iter() {
                                println!("Promoting wrap file {}", file);
                                let mut command = Command::new("meson");
                                command.current_dir(source_path)
                                    .arg("wrap")
                                    .arg("promote")
                                    .arg(file);

                                execute_build_command(&mut command, format!("failed to execute promote command for {}", file).as_str())?;
                            }

                            execute_setup = true;
                            continue;
                        } else {
                            /* the user don't want to promote anything */
                        }
                    }
                }
                return Err(error);
            }
        }

        /* compile */
        {
            let mut command = Command::new("meson");
            command.arg("compile");
            command.arg("-C");
            command.arg(&build_path);
            execute_build_command(&mut command, "failed to execute build")?;
        }

        /* install */
        {
            let mut command = Command::new("meson");
            command.arg("install");
            command.arg("-C");
            command.arg(&build_path);
            let (stdout, stderr) = execute_build_command(&mut command, "failed to install build")?;

            let install_lines = stdout.lines()
                .filter(|line| line.starts_with("Installing "));

            let mut installed_elements = HashMap::with_capacity(50);
            for full_line in install_lines {
                /* cut of the "Installing " part */
                let line = &full_line[11..];
                let mut elements = line.split(" to ");

                let key = elements.next().map(|e| e.to_owned());
                let value = elements.next().map(|e| e.to_owned());
                if elements.next().is_some() {
                    return Err(BuildStepError::new(format!("Meson line \"{}\" contains more than one \" to \" parts.", full_line).to_owned(), stdout, stderr));
                }
                if key.is_none() || value.is_none() {
                    return Err(BuildStepError::new(format!("Meson line \"{}\" misses the key or value.", full_line).to_owned(), stdout, stderr));
                }

                installed_elements.insert(key.unwrap(), value.unwrap());
            }

            /* Gather installed libraries and emit them to the build result */
            //println!("Stdout:\n{}\nStderr:\n{}", stdout.replace("\\", "/"), stderr);
            installed_elements.iter().for_each(|(key, value)| {
                let source = PathBuf::from(key);
                if let Some(extension) = source.extension().map(|e| e.to_string_lossy().into_owned()) {
                    let target = PathBuf::from(value);
                    if !target.is_dir() {
                        eprintln!("meson printed install for file \"{:?}\" to \"{:?}\", but target isn't a directory.", source, target);
                        return;
                    }

                    //println!("Installed {:?} ({}) to {:?}", source, extension, target);
                    if matches!(extension.as_ref(), "a" | "lib") {
                        result.add_library(source.file_name().expect("missing source file name").to_string_lossy().into_owned(), Some(LibraryType::Static));
                    } else if matches!(extension.as_ref(), "so" | "dll") {
                        result.add_library(source.file_name().expect("missing source file name").to_string_lossy().into_owned(), Some(LibraryType::Shared));
                    } else {
                        return;
                    }
                    result.add_library_path(target, Some(LinkSearchKind::Native));
                }
            });
        }

        Ok(())
    }
}

pub struct MesonBuildBuilder {
    inner: MesonBuild
}

impl MesonBuildBuilder {
    fn new() -> Self {
        MesonBuildBuilder{
            inner: MesonBuild{
                callback_promote: None,
                meson_options: HashMap::new()
            }
        }
    }

    pub fn meson_option<K, V>(mut self, key: K, value: V) -> Self
        where K: Into<String>,
              V: Into<String>
    {
        self.inner.meson_options.insert(key.into(), value.into());
        self
    }

    pub fn promote_callback<F: 'static>(mut self, callback: F) -> Self
        where F: Fn(&str) -> Vec<String>
    {
        self.inner.callback_promote = Some(Box::new(callback));
        self
    }

    pub fn build(self) -> MesonBuild {
        self.inner
    }
}

#[cfg(test)]
mod test {
    use crate::build::{BuildBuilder, MesonBuild};
    use crate::source::BuildSourceGit;
    use std::env;
    use crate::Build;

    #[test]
    fn test_build_srtp() {
        let base_url = std::env::current_dir().expect("missing current dir").join("__test_meson");

        env::set_var("rbuild_eson-test-srtp_library_type", "static");

        let source = BuildSourceGit::builder("https://github.com/cisco/libsrtp.git".to_owned())
            .checkout_folder(Some(base_url.clone()))
            .skip_revision_checkout(true)
            .build();

        let meson_step = MesonBuild::builder()
            .meson_option("sctp_build_programs", "false")
            .build();

        /* FIXME: Use some kind of dummy system here! */
        let build = BuildBuilder::new()
            .name("meson-test-srtp")
            .source(Box::new(source))
            .install_prefix(base_url.join("install_root"))
            .build_path(base_url.clone())
            .add_step(Box::new(meson_step))
            .remove_build_dir(false)
            .build();

        let mut build = build.expect("failed to create build");
        match build.execute() {
            Err(error) => {
                println!("{}", error.pretty_format());
                panic!();
            },
            Ok(result) => result.emit_cargo()
        }
    }

    #[test]
    fn test_build_libnice() {
        let base_url = std::env::current_dir().expect("missing current dir").join("__test_meson");

        let source = BuildSourceGit::builder("https://github.com/WolverinDEV/libnice.git".to_owned())
            .checkout_folder(Some(base_url.clone()))
            .build();

        let meson = MesonBuild::builder()
            .promote_callback(|source| {
                println!("Callback promote for {:?}", source);
                vec![
                    "subprojects/glib-2.64.2/subprojects/zlib.wrap".to_owned(),
                    "subprojects/glib-2.64.2/subprojects/libffi.wrap".to_owned(),
                    "subprojects/glib-2.64.2/subprojects/proxy-libintl.wrap".to_owned()
                ]
            })
            .meson_option("gstreamer", "disabled")
            .meson_option("tests", "disabled")
            .build();

        let mut build_builder = Build::builder()
            .name("libnice")
            .source(Box::new(source))
            .add_step(Box::new(meson))
            .build_path(base_url.clone())
            .install_prefix(base_url.join("install_root_nice"))
            .remove_build_dir(false);

        let mut build = build_builder.build().expect("failed to create build");
        match build.execute() {
            Err(error) => {
                println!("{}", error.pretty_format());
                panic!();
            },
            Ok(result) => result.emit_cargo()
        }
    }
}