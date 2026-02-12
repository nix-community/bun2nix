use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Marker substring in bun cache directory entry names.
const BUN_CACHE_ENTRY_MARKER: &str = "@@@";

/// Build an index mapping `"name@version"` to the absolute cache path.
///
/// Scans `cache_dir` for entries containing `@@@` in their name (bun's cache
/// entry marker), reads `package.json` from each to extract name and version.
pub fn build_cache_index(cache_dir: &Path) -> HashMap<String, PathBuf> {
    let mut index = HashMap::new();
    scan_dir(cache_dir, &mut index);
    index
}

fn scan_dir(dir: &Path, index: &mut HashMap<String, PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "Warning: failed to read cache directory {}: {e}",
                dir.display()
            );
            return;
        }
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Scoped package directory (e.g. @babel/) — recurse into it.
        if name_str.starts_with('@') && entry.file_type().is_ok_and(|ft| ft.is_dir()) {
            scan_dir(&entry.path(), index);
            continue;
        }

        // Cache entries contain @@@
        if !name_str.contains(BUN_CACHE_ENTRY_MARKER) {
            continue;
        }

        let full_path = entry.path();
        let pkg_json_path = full_path.join("package.json");

        let pkg_json = match fs::read_to_string(&pkg_json_path) {
            Ok(contents) => contents,
            Err(_) => continue,
        };

        let parsed: serde_json::Value = match serde_json::from_str(&pkg_json) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Warning: failed to parse {}: {e}", pkg_json_path.display());
                continue;
            }
        };

        let pkg_name = match parsed.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => continue,
        };
        let pkg_version = match parsed.get("version").and_then(|v| v.as_str()) {
            Some(v) => v,
            None => continue,
        };

        let key = format!("{pkg_name}@{pkg_version}");
        index.insert(key, full_path);
    }
}
