use thiserror::Error;

/// Result alias for Errors which occur in `bun-rs`
pub type Result<T> = std::result::Result<T, Error>;

#[allow(missing_docs)]
#[derive(Error, Debug)]
/// Errors which occur during bun lockfile parsing
pub enum Error {
    #[error(
        "Failed to parse lockfile as JSONC (specified here: https://github.com/oven-sh/bun/issues/11863): \n{0}.

Please make sure your bun lockfile is formatted correctly, try deleting it and running `bun install` again to produce a fresh one"
    )]
    ParseJsonc(#[from] jsonc_parser::errors::ParseError),
    #[error("Failed to parse lockfile related JSON as rust type: \n{0}")]
    ParseRustType(#[from] serde_json::Error),
    #[error(
        "Failed to parse empty lockfile, make sure you are providing a file with text contents"
    )]
    NoJsoncValue,
    #[error("Missing @ for package name and version declaration.

Make sure all versions in your bun lockfile are formatted properly or try deleting it and running `bun install` to produce a fresh one"
    )]
    NoAtInPackageIdentifier,
    #[error( "Unsupported lockfile version: '{0}'.

Consider updating your local package or contributing to `bun2nix` if this version hasn't been supported yet"
    )]
    UnsupportedLockfileVersion(u8),
    #[error("A workspace package was missing the `workspace:` specifier")]
    MissingWorkspaceSpecifier,
    #[error("A file package was missing the `file:` specifier")]
    MissingFileSpecifier,
    #[error("A git url was missing it's ref")]
    MissingGitRef,
    #[error("A github url was formatted incorrectly")]
    ImproperGithubUrl,
    #[error("Unexpected package entry length: \n{0}")]
    UnexpectedPackageEntryLength(usize),
}
