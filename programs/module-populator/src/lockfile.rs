use std::collections::HashMap;

use bun_rs::lockfile::parse_to_value;
use serde_json::Value;

/// A package entry parsed from the bun.lock `packages` map.
pub struct PopulatorPackage {
    /// Lockfile key (e.g., "vite/esbuild/@esbuild/linux-x64")
    pub key: String,
    /// Resolved identifier (e.g., "@esbuild/linux-x64@0.25.12")
    pub resolved: String,
    /// Package type
    pub kind: PackageKind,
    /// Metadata from the tuple
    pub meta: Option<PackageMeta>,
}

pub enum PackageKind {
    Npm,
    Workspace { path: String },
}

pub struct PackageMeta {
    pub os: Option<String>,
    pub cpu: Option<String>,
    pub bin: Option<BinField>,
}

#[derive(Clone)]
pub enum BinField {
    Single(String),
    Map(HashMap<String, String>),
}

/// Parse a bun.lock file into a list of packages for the module populator.
///
/// Uses `bun_rs::lockfile::parse_to_value()` for JSONC parsing, then
/// walks the `packages` Value manually.
pub fn parse_lockfile(contents: &str) -> Result<Vec<PopulatorPackage>, bun_rs::Error> {
    let value = parse_to_value(contents)?;

    let packages = match value.get("packages").and_then(|v| v.as_object()) {
        Some(p) => p,
        None => return Ok(Vec::new()),
    };

    let mut result = Vec::new();

    for (key, tuple) in packages {
        let arr = match tuple.as_array() {
            Some(a) if !a.is_empty() => a,
            _ => continue,
        };

        let resolved = match arr[0].as_str() {
            Some(s) => s.to_string(),
            None => continue,
        };

        // Workspace packages
        if let Some(ws_path) = parse_workspace_path(&resolved) {
            let path = ws_path.to_string();
            result.push(PopulatorPackage {
                key: key.clone(),
                resolved,
                kind: PackageKind::Workspace { path },
                meta: None,
            });
            continue;
        }

        // Need at least 4 elements for an npm package tuple
        if arr.len() < 4 {
            continue;
        }

        // Parse metadata from the third element (index 2)
        let meta = parse_meta(&arr[2]);

        result.push(PopulatorPackage {
            key: key.clone(),
            resolved,
            kind: PackageKind::Npm,
            meta,
        });
    }

    Ok(result)
}

/// Extract workspace relative path from a resolved string.
/// "@workspace/lib@workspace:packages/lib" -> Some("packages/lib")
fn parse_workspace_path(resolved: &str) -> Option<&str> {
    let marker = "@workspace:";
    let idx = resolved.find(marker)?;
    Some(&resolved[idx + marker.len()..])
}

/// Parse the metadata object from a package tuple element.
fn parse_meta(value: &Value) -> Option<PackageMeta> {
    let obj = value.as_object()?;

    let os = obj.get("os").and_then(|v| v.as_str()).map(String::from);
    let cpu = obj.get("cpu").and_then(|v| v.as_str()).map(String::from);
    let bin = obj.get("bin").map(parse_bin_field);

    Some(PackageMeta { os, cpu, bin })
}

/// Parse the `bin` field which can be a string or an object.
fn parse_bin_field(value: &Value) -> BinField {
    match value {
        Value::String(s) => BinField::Single(s.clone()),
        Value::Object(obj) => {
            let map: HashMap<String, String> = obj
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect();
            BinField::Map(map)
        }
        _ => BinField::Single(String::new()),
    }
}

/// Extract package name from the resolved string.
/// "esbuild@0.25.12" -> Some("esbuild")
/// "@babel/code-frame@7.29.0" -> Some("@babel/code-frame")
pub fn parse_resolved_name(resolved: &str) -> Option<&str> {
    if resolved.contains("@workspace:") {
        return None;
    }
    let last_at = resolved.rfind('@')?;
    if last_at == 0 {
        return None;
    }
    Some(&resolved[..last_at])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_workspace_path() {
        assert_eq!(
            parse_workspace_path("@skeptiva/chromium@workspace:chromium"),
            Some("chromium")
        );
        assert_eq!(
            parse_workspace_path("@workspace/lib@workspace:packages/lib"),
            Some("packages/lib")
        );
        assert_eq!(parse_workspace_path("esbuild@0.25.12"), None);
    }

    #[test]
    fn test_parse_resolved_name() {
        assert_eq!(parse_resolved_name("esbuild@0.25.12"), Some("esbuild"));
        assert_eq!(
            parse_resolved_name("@babel/code-frame@7.29.0"),
            Some("@babel/code-frame")
        );
        assert_eq!(
            parse_resolved_name("@skeptiva/chromium@workspace:chromium"),
            None
        );
    }

    #[test]
    fn test_parse_lockfile_basic() {
        let lockfile = r#"{
            "lockfileVersion": 1,
            "workspaces": {},
            "packages": {
                "typescript": ["typescript@5.7.3", "", { "bin": { "tsc": "bin/tsc", "tsserver": "bin/tsserver" } }, "sha512-abc=="],
                "my-lib": ["my-lib@workspace:packages/my-lib"]
            }
        }"#;

        let packages = parse_lockfile(lockfile).unwrap();
        assert_eq!(packages.len(), 2);

        let ts = packages.iter().find(|p| p.key == "typescript").unwrap();
        assert_eq!(ts.resolved, "typescript@5.7.3");
        assert!(matches!(ts.kind, PackageKind::Npm));
        let meta = ts.meta.as_ref().unwrap();
        assert!(meta.bin.is_some());

        let lib = packages.iter().find(|p| p.key == "my-lib").unwrap();
        assert!(matches!(lib.kind, PackageKind::Workspace { .. }));
    }
}
