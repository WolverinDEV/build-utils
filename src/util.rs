use std::env;
use crate::build::BuildLibraryTarget;
use std::env::VarError;

pub enum BuildLibraryTypeError {
    NotPresent,
    InvalidValue
}

fn parse_build_library_type(library_type: &str) -> Option<BuildLibraryTarget> {
    let library_type = library_type.to_lowercase();
    match library_type.as_ref() {
        "static" => Some(BuildLibraryTarget::Static),
        "shared" => Some(BuildLibraryTarget::Shared),
        _ => None
    }
}

pub fn build_library_type(build_name: &str) -> Result<BuildLibraryTarget, BuildLibraryTypeError> {
    let name = format!("rbuild_{}_library_type", build_name);
    if let Ok(value) = env::var(&name) {
        parse_build_library_type(&value)
            .ok_or(BuildLibraryTypeError::InvalidValue)
    } else if let Ok(value) = env::var("rbuild_library_type") {
        parse_build_library_type(&value)
            .ok_or(BuildLibraryTypeError::InvalidValue)
    } else {
        Err(BuildLibraryTypeError::NotPresent)
    }
}