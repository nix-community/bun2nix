use crate::lockfile::PackageMeta;

/// Check whether the given package metadata matches the current build platform.
///
/// bun.lock uses Node.js-style platform names (e.g. `darwin`, `x64`),
/// which differ from Rust's `std::env::consts` values.
pub fn matches_platform(meta: &PackageMeta) -> bool {
    if let Some(ref os) = meta.os {
        let current_os = map_os(std::env::consts::OS);
        if let Some(current) = current_os {
            if os != current {
                return false;
            }
        }
    }

    if let Some(ref cpu) = meta.cpu {
        let current_cpu = map_cpu(std::env::consts::ARCH);
        if let Some(current) = current_cpu {
            if cpu != current {
                return false;
            }
        }
    }

    true
}

/// Map Rust OS name to bun.lock OS name.
fn map_os(rust_os: &str) -> Option<&str> {
    match rust_os {
        "linux" => Some("linux"),
        "macos" => Some("darwin"),
        "windows" => Some("win32"),
        "freebsd" => Some("freebsd"),
        "openbsd" => Some("openbsd"),
        _ => None,
    }
}

/// Map Rust ARCH name to bun.lock CPU name.
fn map_cpu(rust_arch: &str) -> Option<&str> {
    match rust_arch {
        "x86_64" => Some("x64"),
        "aarch64" => Some("arm64"),
        "arm" => Some("arm"),
        "x86" => Some("ia32"),
        "powerpc64" => Some("ppc64"),
        "s390x" => Some("s390x"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_constraints() {
        let meta = PackageMeta {
            os: None,
            cpu: None,
            bin: None,
        };
        assert!(matches_platform(&meta));
    }

    #[test]
    fn test_map_os() {
        assert_eq!(map_os("linux"), Some("linux"));
        assert_eq!(map_os("macos"), Some("darwin"));
    }

    #[test]
    fn test_map_cpu() {
        assert_eq!(map_cpu("x86_64"), Some("x64"));
        assert_eq!(map_cpu("aarch64"), Some("arm64"));
    }
}
