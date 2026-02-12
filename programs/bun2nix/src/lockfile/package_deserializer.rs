use crate::{Package, error::Result, package::Fetcher};

pub use bun_rs::string_utils::{Values, drop_prefix, split_once_owned, swap_remove_value};

mod prefetch;
pub use prefetch::Prefetch;

/// # Package Deserializer
///
/// Deserializes a given bun lockfile entry line into it's
/// name and nix fetcher implementation
#[derive(Debug)]
pub struct PackageDeserializer {
    /// The name for the package
    pub name: String,

    /// The list of serde json values for the tuple in question
    pub values: Values,
}

impl PackageDeserializer {
    /// # Deserialize package
    ///
    /// Deserialize a given package from it's lockfile representation
    pub fn deserialize_package(name: String, values: Values) -> Result<Package> {
        let arity = values.len();
        let deserializer = Self { name, values };

        match arity {
            1 => deserializer.deserialize_workspace_package(),
            2 => deserializer.deserialize_tarball_or_file_package(),
            3 => deserializer.deserialize_git_or_github_package(),
            4 => deserializer.deserialize_npm_package(),
            x => Err(bun_rs::Error::UnexpectedPackageEntryLength(x).into()),
        }
    }

    /// # Deserialize an NPM Package
    ///
    /// Deserialize an npm package from it's bun lockfile representation
    ///
    /// This is found in the source as a tuple of arity 4:
    /// `[identifier, tarball_url, metadata, hash]`
    ///
    /// The tarball_url field is empty for the default registry (registry.npmjs.org),
    /// or contains the exact URL to the package tarball for non-default registries.
    pub fn deserialize_npm_package(mut self) -> Result<Package> {
        // The bun.lock format for npm packages is:
        // [identifier, tarball_url, metadata, hash]
        // - identifier: "name@version"
        // - tarball_url: "" for default registry, or exact URL to tarball
        // - metadata: object with dependencies, bin, etc.
        // - hash: integrity hash (sha512-...)

        let npm_identifier_raw = swap_remove_value(&mut self.values, 0);
        // After swap_remove(0): [hash, tarball_url, meta]

        let hash = swap_remove_value(&mut self.values, 0);
        // After swap_remove(0): [meta, tarball_url]

        // Get the tarball URL from what's now at index 1
        // (originally at index 1, but the metadata swapped in at index 0)
        let tarball_url = self
            .values
            .get(1)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());

        debug_assert!(
            hash.contains("sha512-"),
            "Expected hash to be in sri format and contain sha512"
        );

        let fetcher = Fetcher::new_npm_package(&npm_identifier_raw, hash, tarball_url)?;

        Ok(Package::new(npm_identifier_raw, fetcher))
    }

    /// # Deserialize a Git or Github Package
    ///
    /// Deserialize a git or github package from it's bun lockfile representation
    ///
    /// This is found in the source as a tuple of arity 3
    pub fn deserialize_git_or_github_package(mut self) -> Result<Package> {
        let mut id = swap_remove_value(&mut self.values, 0);

        let at_pos = id
            .rfind('@')
            .ok_or(bun_rs::Error::NoAtInPackageIdentifier)?;
        id.drain(..=at_pos);

        if id.starts_with("github:") {
            Self::deserialize_github_package(id)
        } else {
            Self::deserialize_git_package(id)
        }
    }

    /// # Deserialize a Github Package
    ///
    /// Deserialize a github package from it's bun lockfile representation
    ///
    /// This is found in the source as a tuple of arity 3
    pub fn deserialize_github_package(id: String) -> Result<Package> {
        let (url, rev) = split_once_owned(id, '#').ok_or(bun_rs::Error::MissingGitRef)?;

        let prefetch_url = format!("{}?ref={}", &url, &rev);
        let prefetch = Prefetch::prefetch_package(&prefetch_url)?;

        let (owner_with_pre, repo) =
            split_once_owned(url, '/').ok_or(bun_rs::Error::ImproperGithubUrl)?;
        let owner = drop_prefix(owner_with_pre, "github:");

        let id_with_ver = format!("github:{}-{}-{}", &owner, &repo, &rev);

        let fetcher = Fetcher::FetchGitHub {
            owner,
            repo,
            rev,
            hash: prefetch.hash,
        };

        Ok(Package::new(id_with_ver, fetcher))
    }

    /// # Deserialize a Git Package
    ///
    /// Deserialize a git package from it's bun lockfile representation
    ///
    /// This is found in the source as a tuple of arity 3
    pub fn deserialize_git_package(id: String) -> Result<Package> {
        let git_url = drop_prefix(id, "git+");
        let (url, rev) = split_once_owned(git_url, '#').ok_or(bun_rs::Error::MissingGitRef)?;

        let prefetch_url = format!("git+{}?rev={}", &url, &rev);
        let prefetch = Prefetch::prefetch_package(&prefetch_url)?;

        let id_with_rev = format!("git:{}", &rev);

        let fetcher = Fetcher::FetchGit {
            url,
            rev,
            hash: prefetch.hash,
        };

        Ok(Package::new(id_with_rev, fetcher))
    }

    /// # Deserialize a tarball or file package
    ///
    /// Deserialize a tarball or file package from it's bun
    /// lockfile representation
    ///
    /// These are grouped together as both lockfile
    /// representations are a tupe of arity 2, hence
    /// paths starting with `http` are considered
    /// tarballs
    pub fn deserialize_tarball_or_file_package(mut self) -> Result<Package> {
        let id = swap_remove_value(&mut self.values, 0);
        let path =
            Self::drain_after_substring(id, "@").ok_or(bun_rs::Error::NoAtInPackageIdentifier)?;

        if path.starts_with("http") {
            Self::deserialize_tarball_package(path)
        } else {
            Self::deserialize_file_package(self.name, path)
        }
    }

    /// # Deserialize a file package
    ///
    /// Deserialize a file package from it's bun lockfile representation
    ///
    /// This is found in the source as a tuple of arity 2
    pub fn deserialize_file_package(name: String, path: String) -> Result<Package> {
        debug_assert!(
            !path.contains("http"),
            "File path can never contain http, because then it would be a tarball"
        );

        let path = Self::drain_after_substring(path, "file:")
            .ok_or(bun_rs::Error::MissingFileSpecifier)?;

        Ok(Package::new(name, Fetcher::CopyToStore { path }))
    }

    /// # Deserialize a tarball package
    ///
    /// Deserialize a tarball package from it's bun lockfile representation
    ///
    /// This is found in the source as a tuple of arity 2
    pub fn deserialize_tarball_package(url: String) -> Result<Package> {
        debug_assert!(url.contains("http"), "Expected tarball url to contain http");

        let prefetch = Prefetch::prefetch_package(&url)?;

        let name = format!("tarball:{}", url);
        let fetcher = Fetcher::FetchTarball {
            url,
            hash: prefetch.hash,
        };

        Ok(Package::new(name, fetcher))
    }

    /// # Deserialize a workspace package
    ///
    /// Deserialize a workspace package from it's bun lockfile representation
    ///
    /// This is found in the source as a tuple of arity 2
    pub fn deserialize_workspace_package(mut self) -> Result<Package> {
        let id = swap_remove_value(&mut self.values, 0);
        let path = Self::drain_after_substring(id, "workspace:")
            .ok_or(bun_rs::Error::MissingWorkspaceSpecifier)?;

        Ok(Package::new(self.name, Fetcher::CopyToStore { path }))
    }

    fn drain_after_substring(mut input: String, sub: &str) -> Option<String> {
        let pos = input.rfind(sub)? + sub.len();

        Some(input.drain(pos..).collect())
    }
}
