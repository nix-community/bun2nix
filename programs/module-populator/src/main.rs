mod cache_index;
mod key_path;
mod lockfile;
mod node_modules;
mod platform;

use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;
use thiserror::Error;

use cache_index::build_cache_index;
use key_path::{get_node_modules_path, parse_key_path};
use lockfile::{PackageKind, parse_lockfile, parse_resolved_name};
use node_modules::{copy_package, create_bin_links, symlink_workspace};
use platform::matches_platform;

#[derive(Error, Debug)]
enum Error {
    #[error("Error reading lockfile: {0}")]
    ReadLockfile(#[from] std::io::Error),
    #[error("Error parsing lockfile: {0}")]
    ParseLockfile(#[from] bun_rs::Error),
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Parser)]
#[command(name = "module-populator")]
#[command(about = "Construct node_modules from bun cache without network access")]
struct Cli {
    /// Path to bun.lock
    #[arg(short, long, default_value = "./bun.lock")]
    lock_file: PathBuf,

    /// Path to bun cache directory
    #[arg(short, long, env = "BUN_INSTALL_CACHE_DIR")]
    cache_dir: PathBuf,

    /// Root directory for node_modules output
    #[arg(short, long, default_value = ".")]
    out_dir: PathBuf,
}

struct BinEntry {
    package_path: PathBuf,
    bin_field: lockfile::BinField,
    bin_dir: PathBuf,
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    let lock_contents = fs::read_to_string(&cli.lock_file)?;

    println!("Reading lockfile: {}", cli.lock_file.display());
    let packages = parse_lockfile(&lock_contents)?;

    println!("Scanning cache: {}", cli.cache_dir.display());
    let cache_index = build_cache_index(&cli.cache_dir);
    println!("Cache index: {} entries", cache_index.len());

    let out_dir = &cli.out_dir;
    let mut linked: usize = 0;
    let mut skipped_platform: usize = 0;
    let mut skipped_other: usize = 0;
    let mut not_found: usize = 0;

    let mut bin_entries: Vec<BinEntry> = Vec::new();

    for pkg in &packages {
        match &pkg.kind {
            PackageKind::Workspace { path } => {
                let key_path = parse_key_path(&pkg.key);
                let target_path = get_node_modules_path(out_dir, &key_path);
                let absolute_workspace_path =
                    fs::canonicalize(out_dir.join(path)).unwrap_or_else(|_| out_dir.join(path));

                let canonical_out =
                    fs::canonicalize(out_dir).unwrap_or_else(|_| out_dir.to_path_buf());
                assert!(
                    absolute_workspace_path.starts_with(&canonical_out),
                    "workspace path escapes output directory: {} is not under {}",
                    absolute_workspace_path.display(),
                    canonical_out.display()
                );

                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent).ok();
                }

                symlink_workspace(&absolute_workspace_path, &target_path);
                linked += 1;
            }
            PackageKind::Npm => {
                let pkg_name = match parse_resolved_name(&pkg.resolved) {
                    Some(n) => n,
                    None => {
                        skipped_other += 1;
                        continue;
                    }
                };

                // Check platform constraints
                if let Some(ref meta) = pkg.meta {
                    if (meta.os.is_some() || meta.cpu.is_some()) && !matches_platform(meta) {
                        skipped_platform += 1;
                        continue;
                    }
                }

                let cache_path = match cache_index.get(&pkg.resolved) {
                    Some(p) => p,
                    None => {
                        eprintln!(
                            "  Warning: cache miss for {} (key: {})",
                            pkg.resolved, pkg.key
                        );
                        not_found += 1;
                        continue;
                    }
                };

                let key_path = parse_key_path(&pkg.key);
                let target_path = get_node_modules_path(out_dir, &key_path);

                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent).ok();
                }

                copy_package(cache_path, &target_path);
                linked += 1;

                // Collect bin entries for later linking
                if let Some(ref meta) = pkg.meta {
                    if let Some(ref bin) = meta.bin {
                        let nm_dir = if pkg_name.starts_with('@') {
                            // For scoped packages, go up two levels to get node_modules
                            target_path
                                .parent()
                                .and_then(Path::parent)
                                .unwrap_or(out_dir)
                        } else {
                            target_path.parent().unwrap_or(out_dir)
                        };
                        let bin_dir = nm_dir.join(".bin");

                        bin_entries.push(BinEntry {
                            package_path: target_path.clone(),
                            bin_field: bin.clone(),
                            bin_dir,
                        });
                    }
                }
            }
        }
    }

    // Create all bin links
    for entry in &bin_entries {
        create_bin_links(&entry.package_path, &entry.bin_field, &entry.bin_dir);
    }

    println!(
        "Linked {linked} packages, skipped {skipped_platform} (platform), \
         {skipped_other} (other), {not_found} not found in cache."
    );

    if not_found > 0 {
        eprintln!("Warning: some packages were not found in the cache. The build may fail.");
    }

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
