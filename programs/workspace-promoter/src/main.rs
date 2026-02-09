use std::fmt::Write;
use std::fs;
use std::path::PathBuf;

use clap::Parser;
use serde_json::{Map, Value};
use thiserror::Error;

#[derive(Error, Debug)]
enum Error {
    #[error("Error reading lockfile: {0}")]
    ReadLockfile(#[from] std::io::Error),
    #[error("Error parsing lockfile: {0}")]
    ParseLockfile(#[from] bun_rs::Error),
    #[error("Lockfile missing \"{0}\" key")]
    MissingKey(&'static str),
    #[error("Workspace \"{0}\" not found in lockfile")]
    WorkspaceNotFound(String),
    #[error("Error parsing package.json: {0}")]
    ParsePackageJson(serde_json::Error),
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Parser)]
#[command(name = "workspace-promoter")]
#[command(about = "Promote a bun workspace member to lockfile root")]
struct Cli {
    /// Workspace key to promote (e.g. "packages/app")
    #[arg(short, long)]
    workspace: String,

    /// Workspace dependency names to strip (repeatable)
    #[arg(long)]
    strip_dep: Vec<String>,

    /// Path to package.json to update with merged sibling deps
    #[arg(long)]
    package_json: Option<PathBuf>,

    /// Path to bun.lock
    lockfile: PathBuf,
}

/// Promote a workspace member to lockfile root, returning the mutated lockfile
/// value and the collected sibling dependencies.
fn promote(root: &mut Value, workspace: &str, strip_deps: &[String]) -> Result<Map<String, Value>> {
    let workspaces = root
        .get_mut("workspaces")
        .and_then(Value::as_object_mut)
        .ok_or(Error::MissingKey("workspaces"))?;

    // Collect sibling dependencies before we discard their entries.
    let mut sibling_deps: Map<String, Value> = Map::new();
    for dep_name in strip_deps {
        // Find the workspace entry whose .name matches this dep.
        let ws_deps = workspaces
            .values()
            .filter_map(Value::as_object)
            .find(|ws| ws.get("name").and_then(Value::as_str) == Some(dep_name))
            .and_then(|ws| ws.get("dependencies"))
            .and_then(Value::as_object);

        if let Some(deps) = ws_deps {
            for (k, v) in deps {
                sibling_deps.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
    }

    // Remove stripped workspace deps from sibling deps to prevent them from
    // being re-added during the merge step.
    for dep_name in strip_deps {
        sibling_deps.remove(dep_name.as_str());
    }

    // Take the target workspace value out.
    let promoted = workspaces
        .remove(workspace)
        .ok_or_else(|| Error::WorkspaceNotFound(workspace.to_string()))?;

    // Clear all workspaces and insert promoted as root (empty-string key).
    workspaces.clear();
    workspaces.insert(String::new(), promoted);

    // Strip workspace deps from the promoted root's dependencies.
    if let Some(deps) = workspaces
        .get_mut("")
        .and_then(Value::as_object_mut)
        .and_then(|ws| ws.get_mut("dependencies"))
        .and_then(Value::as_object_mut)
    {
        for dep_name in strip_deps {
            deps.remove(dep_name);
        }
    }

    // Remove workspace package entries from .packages.
    if let Some(packages) = root.get_mut("packages").and_then(Value::as_object_mut) {
        packages.retain(|_key, val| {
            // Each package entry is an array; the first element (index 0)
            // is the resolved identifier (e.g. "@workspace/app@workspace:packages/app").
            // Drop entries containing "workspace:".
            val.as_array()
                .and_then(|arr| arr.first())
                .and_then(Value::as_str)
                .map_or(true, |resolved| !resolved.contains("workspace:"))
        });
    }

    // Merge sibling deps into the promoted root (don't overwrite existing).
    if !sibling_deps.is_empty() {
        if let Some(deps) = root
            .pointer_mut("/workspaces//dependencies")
            .and_then(Value::as_object_mut)
        {
            for (k, v) in &sibling_deps {
                deps.entry(k.clone()).or_insert(v.clone());
            }
        } else {
            // No dependencies map yet — create one.
            if let Some(ws) = root
                .pointer_mut("/workspaces/")
                .and_then(Value::as_object_mut)
            {
                ws.insert(
                    "dependencies".to_string(),
                    Value::Object(sibling_deps.clone()),
                );
            }
        }
    }

    Ok(sibling_deps)
}

/// Merge sibling dependencies into a package.json value (does not overwrite
/// existing entries).
fn merge_deps_into_package_json(pkg: &mut Value, sibling_deps: &Map<String, Value>) {
    let deps = pkg
        .as_object_mut()
        .map(|obj| {
            obj.entry("dependencies")
                .or_insert_with(|| Value::Object(Map::new()))
        })
        .and_then(Value::as_object_mut);

    if let Some(deps) = deps {
        for (k, v) in sibling_deps {
            deps.entry(k.clone()).or_insert(v.clone());
        }
    }
}

/// Serialise a JSON value to bun's JSONC lockfile format: 2-space
/// indentation with trailing commas after every array element and
/// object entry.
fn to_jsonc(value: &Value) -> String {
    let mut buf = String::new();
    write_jsonc_value(&mut buf, value, 0);
    buf.push('\n');
    buf
}

fn write_jsonc_value(buf: &mut String, value: &Value, depth: usize) {
    match value {
        Value::Null => buf.push_str("null"),
        Value::Bool(b) => write!(buf, "{b}").unwrap(),
        Value::Number(n) => write!(buf, "{n}").unwrap(),
        Value::String(s) => {
            buf.push('"');
            for ch in s.chars() {
                match ch {
                    '"' => buf.push_str("\\\""),
                    '\\' => buf.push_str("\\\\"),
                    '\n' => buf.push_str("\\n"),
                    '\r' => buf.push_str("\\r"),
                    '\t' => buf.push_str("\\t"),
                    c if c.is_control() => write!(buf, "\\u{:04x}", c as u32).unwrap(),
                    c => buf.push(c),
                }
            }
            buf.push('"');
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                buf.push_str("[]");
                return;
            }
            buf.push_str("[\n");
            for item in arr {
                write_indent(buf, depth + 1);
                write_jsonc_value(buf, item, depth + 1);
                buf.push_str(",\n");
            }
            write_indent(buf, depth);
            buf.push(']');
        }
        Value::Object(map) => {
            if map.is_empty() {
                buf.push_str("{}");
                return;
            }
            buf.push_str("{\n");
            for (key, val) in map {
                write_indent(buf, depth + 1);
                write!(buf, "\"{key}\": ").unwrap();
                write_jsonc_value(buf, val, depth + 1);
                buf.push_str(",\n");
            }
            write_indent(buf, depth);
            buf.push('}');
        }
    }
}

fn write_indent(buf: &mut String, depth: usize) {
    for _ in 0..depth {
        buf.push_str("  ");
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    let contents = fs::read_to_string(&cli.lockfile)?;
    let mut root: Value = bun_rs::lockfile::parse_to_value(&contents)?;

    let sibling_deps = promote(&mut root, &cli.workspace, &cli.strip_dep)?;

    // Also update package.json so bun install doesn't discard the lockfile.
    if !sibling_deps.is_empty() {
        if let Some(ref pkg_path) = cli.package_json {
            let pkg_contents = fs::read_to_string(pkg_path)?;
            let mut pkg: Value =
                serde_json::from_str(&pkg_contents).map_err(Error::ParsePackageJson)?;

            merge_deps_into_package_json(&mut pkg, &sibling_deps);

            let pkg_output = serde_json::to_string_pretty(&pkg).unwrap();
            fs::write(pkg_path, format!("{pkg_output}\n"))?;
        }
    }

    let output = to_jsonc(&root);
    fs::write(&cli.lockfile, output)?;

    println!("Promoted workspace \"{}\" to lockfile root.", cli.workspace);

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_lockfile() -> Value {
        json!({
            "lockfileVersion": 1,
            "workspaces": {
                "": {
                    "name": "test-workspace",
                    "devDependencies": {
                        "bun2nix": "^2.0.0"
                    }
                },
                "packages/app": {
                    "name": "@workspace/app",
                    "version": "1.0.0",
                    "dependencies": {
                        "@workspace/lib": "workspace:*",
                        "is-odd": "^3.0.1"
                    }
                },
                "packages/lib": {
                    "name": "@workspace/lib",
                    "version": "1.0.0",
                    "dependencies": {
                        "is-number": "^6.0.0"
                    }
                }
            },
            "packages": {
                "@workspace/app": ["@workspace/app@workspace:packages/app"],
                "@workspace/lib": ["@workspace/lib@workspace:packages/lib"],
                "is-odd": ["is-odd@3.0.1", "", {}, "sha512-abc=="],
                "is-number": ["is-number@6.0.0", "", {}, "sha512-def=="]
            }
        })
    }

    #[test]
    fn promote_replaces_workspaces_with_target() {
        let mut lock = sample_lockfile();
        promote(&mut lock, "packages/app", &[]).unwrap();

        let ws = lock["workspaces"].as_object().unwrap();
        assert_eq!(ws.len(), 1, "should have exactly one workspace entry");
        assert!(
            ws.contains_key(""),
            "promoted entry should be at empty-string key"
        );
        assert_eq!(ws[""]["name"], "@workspace/app");
    }

    #[test]
    fn promote_strips_workspace_packages() {
        let mut lock = sample_lockfile();
        promote(&mut lock, "packages/app", &[]).unwrap();

        let packages = lock["packages"].as_object().unwrap();
        assert!(!packages.contains_key("@workspace/app"));
        assert!(!packages.contains_key("@workspace/lib"));
        assert!(packages.contains_key("is-odd"));
        assert!(packages.contains_key("is-number"));
    }

    #[test]
    fn promote_strips_dep_from_promoted_root() {
        let mut lock = sample_lockfile();
        let strip = vec!["@workspace/lib".to_string()];
        promote(&mut lock, "packages/app", &strip).unwrap();

        let deps = lock["workspaces"][""]["dependencies"].as_object().unwrap();
        assert!(
            !deps.contains_key("@workspace/lib"),
            "workspace dep should be stripped"
        );
        assert!(
            deps.contains_key("is-odd"),
            "non-workspace dep should remain"
        );
    }

    #[test]
    fn promote_merges_sibling_deps() {
        let mut lock = sample_lockfile();
        let strip = vec!["@workspace/lib".to_string()];
        let sibling = promote(&mut lock, "packages/app", &strip).unwrap();

        // Sibling deps from @workspace/lib should be collected.
        assert!(sibling.contains_key("is-number"));

        // They should be merged into the promoted root's dependencies.
        let deps = lock["workspaces"][""]["dependencies"].as_object().unwrap();
        assert!(
            deps.contains_key("is-number"),
            "sibling dep should be merged"
        );
    }

    #[test]
    fn promote_does_not_overwrite_existing_deps() {
        let mut lock = sample_lockfile();
        // Both app and lib depend on is-odd (app directly, lib doesn't here but
        // let's add it to test the no-overwrite behaviour).
        lock["workspaces"]["packages/lib"]["dependencies"]["is-odd"] = json!("^999.0.0");

        let strip = vec!["@workspace/lib".to_string()];
        promote(&mut lock, "packages/app", &strip).unwrap();

        let deps = lock["workspaces"][""]["dependencies"].as_object().unwrap();
        // The promoted root already had is-odd@^3.0.1; the sibling's ^999.0.0
        // should NOT overwrite it.
        assert_eq!(deps["is-odd"], "^3.0.1");
    }

    #[test]
    fn promote_workspace_not_found() {
        let mut lock = sample_lockfile();
        let err = promote(&mut lock, "packages/nonexistent", &[]).unwrap_err();
        assert!(matches!(err, Error::WorkspaceNotFound(_)));
    }

    #[test]
    fn promote_creates_deps_map_when_absent() {
        let mut lock = json!({
            "lockfileVersion": 1,
            "workspaces": {
                "": { "name": "root" },
                "packages/app": {
                    "name": "@workspace/app",
                    "version": "1.0.0"
                },
                "packages/lib": {
                    "name": "@workspace/lib",
                    "version": "1.0.0",
                    "dependencies": {
                        "is-number": "^6.0.0"
                    }
                }
            },
            "packages": {}
        });

        let strip = vec!["@workspace/lib".to_string()];
        promote(&mut lock, "packages/app", &strip).unwrap();

        // A dependencies map should have been created on the promoted root.
        let deps = lock["workspaces"][""]["dependencies"].as_object().unwrap();
        assert!(deps.contains_key("is-number"));
    }

    #[test]
    fn merge_deps_into_package_json_adds_missing() {
        let mut pkg = json!({
            "name": "@workspace/app",
            "dependencies": {
                "existing": "^1.0.0"
            }
        });

        let mut sibling = Map::new();
        sibling.insert("new-dep".to_string(), json!("^2.0.0"));

        merge_deps_into_package_json(&mut pkg, &sibling);

        let deps = pkg["dependencies"].as_object().unwrap();
        assert_eq!(deps["existing"], "^1.0.0");
        assert_eq!(deps["new-dep"], "^2.0.0");
    }

    #[test]
    fn merge_deps_into_package_json_no_overwrite() {
        let mut pkg = json!({
            "name": "@workspace/app",
            "dependencies": {
                "shared": "^1.0.0"
            }
        });

        let mut sibling = Map::new();
        sibling.insert("shared".to_string(), json!("^9.0.0"));

        merge_deps_into_package_json(&mut pkg, &sibling);

        assert_eq!(pkg["dependencies"]["shared"], "^1.0.0");
    }

    #[test]
    fn promote_does_not_reintroduce_stripped_transitive_deps() {
        // When @workspace/lib depends on @workspace/util (also stripped),
        // @workspace/util should not appear in the promoted root.
        let mut lock = json!({
            "lockfileVersion": 1,
            "workspaces": {
                "": { "name": "root" },
                "packages/app": {
                    "name": "@workspace/app",
                    "version": "1.0.0",
                    "dependencies": {
                        "@workspace/lib": "workspace:*",
                        "is-odd": "^3.0.1"
                    }
                },
                "packages/lib": {
                    "name": "@workspace/lib",
                    "version": "1.0.0",
                    "dependencies": {
                        "@workspace/util": "workspace:*",
                        "is-number": "^6.0.0"
                    }
                },
                "packages/util": {
                    "name": "@workspace/util",
                    "version": "1.0.0"
                }
            },
            "packages": {
                "@workspace/app": ["@workspace/app@workspace:packages/app"],
                "@workspace/lib": ["@workspace/lib@workspace:packages/lib"],
                "@workspace/util": ["@workspace/util@workspace:packages/util"],
                "is-odd": ["is-odd@3.0.1", "", {}, "sha512-abc=="],
                "is-number": ["is-number@6.0.0", "", {}, "sha512-def=="]
            }
        });

        let strip = vec![
            "@workspace/lib".to_string(),
            "@workspace/util".to_string(),
        ];
        promote(&mut lock, "packages/app", &strip).unwrap();

        let deps = lock["workspaces"][""]["dependencies"].as_object().unwrap();
        assert!(
            !deps.contains_key("@workspace/lib"),
            "stripped dep should not appear"
        );
        assert!(
            !deps.contains_key("@workspace/util"),
            "transitive stripped dep should not be re-added"
        );
        assert!(deps.contains_key("is-odd"), "direct dep should remain");
        assert!(
            deps.contains_key("is-number"),
            "sibling non-workspace dep should be merged"
        );
    }

    #[test]
    fn merge_deps_creates_dependencies_key() {
        let mut pkg = json!({ "name": "bare-pkg" });

        let mut sibling = Map::new();
        sibling.insert("new-dep".to_string(), json!("^1.0.0"));

        merge_deps_into_package_json(&mut pkg, &sibling);

        assert_eq!(pkg["dependencies"]["new-dep"], "^1.0.0");
    }
}
