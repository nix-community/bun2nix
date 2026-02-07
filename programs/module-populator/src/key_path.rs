/// Parse a lockfile key into a nesting path of package names.
///
/// Scoped packages (starting with `@`) are treated as a single segment
/// together with the following path component.
///
/// # Examples
///
/// ```text
/// "svelte"                          -> ["svelte"]
/// "@babel/code-frame"               -> ["@babel/code-frame"]
/// "vite/esbuild"                    -> ["vite", "esbuild"]
/// "vite/esbuild/@esbuild/linux-x64" -> ["vite", "esbuild", "@esbuild/linux-x64"]
/// ```
pub fn parse_key_path(key: &str) -> Vec<&str> {
    let segments: Vec<&str> = key.split('/').collect();
    let mut parts = Vec::new();
    let mut i = 0;

    while i < segments.len() {
        if segments[i].starts_with('@') && i + 1 < segments.len() {
            // Find the byte offsets to produce a single &str slice covering "scope/name"
            let start = segments[i].as_ptr() as usize - key.as_ptr() as usize;
            let end =
                segments[i + 1].as_ptr() as usize - key.as_ptr() as usize + segments[i + 1].len();
            parts.push(&key[start..end]);
            i += 2;
        } else {
            parts.push(segments[i]);
            i += 1;
        }
    }

    parts
}

/// Validate that a path segment is safe to use in filesystem operations.
///
/// Rejects segments containing `..` components, null bytes, absolute paths,
/// backslashes, or empty strings. Scoped packages (`@scope/name`) are split
/// on `/` and each sub-component is checked individually.
fn validate_segment(segment: &str) {
    assert!(!segment.is_empty(), "path segment must not be empty");
    assert!(
        !segment.contains('\0'),
        "path segment must not contain null bytes: {segment:?}"
    );
    assert!(
        !segment.contains('\\'),
        "path segment must not contain backslashes: {segment:?}"
    );
    assert!(
        !segment.starts_with('/'),
        "path segment must not be an absolute path: {segment:?}"
    );

    // For scoped packages like @scope/name, check each sub-component
    for component in segment.split('/') {
        assert!(
            !component.is_empty(),
            "path sub-component must not be empty in: {segment:?}"
        );
        assert!(
            component != "..",
            "path segment must not contain '..': {segment:?}"
        );
    }
}

/// Convert a parsed key path into a node_modules filesystem path.
///
/// Each nesting level adds a `node_modules/` prefix:
/// - `["svelte"]` -> `{root}/node_modules/svelte`
/// - `["vite", "esbuild"]` -> `{root}/node_modules/vite/node_modules/esbuild`
///
/// # Panics
///
/// Panics if any segment contains path traversal components (`..`),
/// null bytes, backslashes, absolute paths, or is empty.
pub fn get_node_modules_path(root: &std::path::Path, key_path: &[&str]) -> std::path::PathBuf {
    let mut path = root.to_path_buf();
    for part in key_path {
        validate_segment(part);
        path.push("node_modules");
        path.push(part);
    }
    assert!(
        path.starts_with(root),
        "constructed path escapes root: {} is not under {}",
        path.display(),
        root.display()
    );
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_simple_package() {
        assert_eq!(parse_key_path("svelte"), vec!["svelte"]);
    }

    #[test]
    fn test_scoped_package() {
        assert_eq!(
            parse_key_path("@babel/code-frame"),
            vec!["@babel/code-frame"]
        );
    }

    #[test]
    fn test_nested_package() {
        assert_eq!(parse_key_path("vite/esbuild"), vec!["vite", "esbuild"]);
    }

    #[test]
    fn test_nested_scoped_package() {
        assert_eq!(
            parse_key_path("vite/esbuild/@esbuild/linux-x64"),
            vec!["vite", "esbuild", "@esbuild/linux-x64"]
        );
    }

    #[test]
    fn test_node_modules_path_simple() {
        let root = Path::new("/project");
        assert_eq!(
            get_node_modules_path(root, &["svelte"]),
            Path::new("/project/node_modules/svelte")
        );
    }

    #[test]
    fn test_node_modules_path_nested() {
        let root = Path::new("/project");
        assert_eq!(
            get_node_modules_path(root, &["vite", "esbuild"]),
            Path::new("/project/node_modules/vite/node_modules/esbuild")
        );
    }

    #[test]
    fn test_node_modules_path_nested_scoped() {
        let root = Path::new("/project");
        assert_eq!(
            get_node_modules_path(root, &["vite", "esbuild", "@esbuild/linux-x64"]),
            Path::new(
                "/project/node_modules/vite/node_modules/esbuild/node_modules/@esbuild/linux-x64"
            )
        );
    }

    #[test]
    #[should_panic(expected = "must not contain '..'")]
    fn test_rejects_dotdot_segment() {
        get_node_modules_path(Path::new("/project"), &["../etc"]);
    }

    #[test]
    #[should_panic(expected = "must not contain '..'")]
    fn test_rejects_dotdot_in_scoped() {
        get_node_modules_path(Path::new("/project"), &["@scope/.."]);
    }

    #[test]
    #[should_panic(expected = "must not contain null bytes")]
    fn test_rejects_null_bytes() {
        get_node_modules_path(Path::new("/project"), &["pkg\0evil"]);
    }

    #[test]
    #[should_panic(expected = "must not be an absolute path")]
    fn test_rejects_absolute_path() {
        get_node_modules_path(Path::new("/project"), &["/etc/passwd"]);
    }

    #[test]
    #[should_panic(expected = "must not be empty")]
    fn test_rejects_empty_segment() {
        get_node_modules_path(Path::new("/project"), &[""]);
    }

    #[test]
    #[should_panic(expected = "must not contain backslashes")]
    fn test_rejects_backslashes() {
        get_node_modules_path(Path::new("/project"), &["pkg\\evil"]);
    }
}
