use std::collections::HashMap;

use log::warn;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};

/// # Parse to Value
///
/// Parse a bun lockfile string into a serde json value
pub fn parse_to_value(lockfile: &str) -> Result<Value> {
    jsonc_parser::parse_to_serde_value(lockfile, &Default::default())?.ok_or(Error::NoJsoncValue)
}

/// Dependencies type alias
pub type Dependencies = HashMap<String, String>;

#[derive(Default, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", default)]
/// # Lockfile workspace
///
/// A model of the fields that exist in a given workspace
pub struct Workspace {
    /// The name of the workspace
    pub name: Option<String>,

    /// Dependencies of the workspace
    #[serde(default, deserialize_with = "Workspace::deserialize_dependencies")]
    pub dependencies: Dependencies,

    /// Dev dependencies of the workspace
    #[serde(default, deserialize_with = "Workspace::deserialize_dependencies")]
    pub dev_dependencies: Dependencies,
}

impl Workspace {
    /// # Deserialize Dependencies
    ///
    /// Wraps the default deserialization method in order to add checking for unresolved deps
    pub fn deserialize_dependencies<'de, D>(data: D) -> std::result::Result<Dependencies, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Dependencies::deserialize(data)?
            .into_iter()
            .map(|(name, version)| {
                if version == "latest" {
                    warn!(
                        "
The provided bun lockfile contains an unlocked dependency.

This looks something like:
```json
dependencies: {{
    \"{name}\": \"latest\"
}}
```
As a result, this may not be able to be used as a base to do reproducible
installs off of.

You may fix this by running `bun install` again and allowing
it to pin a specific version, manually inserting a version instead
of \"latest\" or removing the dependency if it is unused.
                "
                    );
                };

                (name, version)
            })
            .collect())
    }
}
