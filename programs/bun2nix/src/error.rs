//! Errors which may occur during the running of this program
//!
//! This module contains two items:
//! - A giant unified error type `Error`
//! - An alias for `std::result::Result<T, E>` with that error for convenience

use std::{io, str::Utf8Error};
use thiserror::Error;

/// Result alias for Errors which occur in `bun2nix`
pub type Result<T> = std::result::Result<T, Error>;

#[allow(missing_docs)]
#[derive(Error, Debug)]
/// Errors which occur in `bun2nix`
pub enum Error {
    #[error(transparent)]
    BunRs(#[from] bun_rs::Error),
    #[error("Error while fetching package from it's source: \n{0}")]
    FetchingFailed(io::Error),
    #[error("\nConsole error while fetching package from it's source: \n\n{0}")]
    FetchingError(String),
    #[error("An invalid utf8 string was returned from stdin while fetching a package: {0}")]
    InvalidUtf8String(Utf8Error),
    #[error("Failed to render template: '\n{0}'")]
    TemplateError(#[from] askama::Error),
    #[error(
        "Hash was not already known for `{0}`.

This must be prefetched and hashed by `bun2nix` via
`nix flake prefetch`. However, you are using the wasm
cli, which does not support this as a child process
needs to be spawned.

Please switch to the native cli instead to use this dependency.
"
    )]
    UnsupportedWASMCliAction(String),
    #[error("IO Error Occurred: \n{0}

Make sure that the bun lockfile path you gave points to a valid path.

Try changing the file path to point to one, or create one with `bun install` on a version of bun above v1.2.

See https://bun.sh/docs/install/lockfile to find out more information about the textual lockfile.

Try `bun2nix -h` for help.
    ")]
    ReadLockfileError(#[from] io::Error),
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;

#[cfg(target_arch = "wasm32")]
impl From<Error> for JsValue {
    fn from(err: Error) -> JsValue {
        JsValue::from_str(&err.to_string())
    }
}
