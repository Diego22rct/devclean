use std::path::{Path, PathBuf};
use crossbeam_channel::{Receiver, Sender};

use crate::scanner::DiscoveredArtifact;

/// Result of sizing an artifact directory or path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SizeResult {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub last_modified: Option<chrono::DateTime<chrono::Local>>,
}

/// Recursively calculates the total byte size of files and most recent modification time in a directory.
///
/// If `path` is a single file, returns its size and modification timestamp.
/// If `path` does not exist or is unreadable, returns `(0, None)`.
/// Subdirectory symlinks are not traversed to avoid cycles and escaping the artifact boundary.
pub fn calculate_dir_size(path: &Path) -> (u64, Option<chrono::DateTime<chrono::Local>>) {
    if !path.exists() {
        return (0, None);
    }

    // Try standard canonicalization to get \\?\ extended-length paths on Windows; fallback to dunce / path
    let canonical = std::fs::canonicalize(path)
        .or_else(|_| dunce::canonicalize(path))
        .unwrap_or_else(|_| path.to_path_buf());

    let meta = match std::fs::symlink_metadata(&canonical) {
        Ok(m) => m,
        Err(_) => return (0, None),
    };

    if meta.is_file() {
        let mod_time = meta.modified().ok().map(|t| t.into());
        return (meta.len(), mod_time);
    }

    if !meta.is_dir() {
        return (0, meta.modified().ok().map(|t| t.into()));
    }

    let mut total_bytes = 0u64;
    let mut max_modified = meta.modified().ok();
    let mut stack = vec![canonical];

    while let Some(dir) = stack.pop() {
        let read_dir = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => continue, // Safely ignore directories that cannot be read (permissions/locks)
        };

        for entry_res in read_dir {
            let entry = match entry_res {
                Ok(e) => e,
                Err(_) => continue,
            };

            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            // Avoid following symlinks to prevent cycles or escaping the artifact root
            if file_type.is_symlink() {
                continue;
            }

            if file_type.is_dir() {
                if let Ok(entry_meta) = entry.metadata() {
                    if let Ok(mod_time) = entry_meta.modified() {
                        max_modified = Some(match max_modified {
                            Some(current) => current.max(mod_time),
                            None => mod_time,
                        });
                    }
                }
                stack.push(entry.path());
            } else if file_type.is_file() {
                if let Ok(entry_meta) = entry.metadata() {
                    total_bytes = total_bytes.saturating_add(entry_meta.len());
                    if let Ok(mod_time) = entry_meta.modified() {
                        max_modified = Some(match max_modified {
                            Some(current) => current.max(mod_time),
                            None => mod_time,
                        });
                    }
                }
            }
        }
    }

    let max_chrono = max_modified.map(|t| t.into());
    (total_bytes, max_chrono)
}

/// Helper to calculate the aggregate size and most recent modification time for multiple paths.
/// Useful for multi-path/aggregated artifacts (e.g. Python `__pycache__` directories).
pub fn calculate_paths_size(paths: &[&Path]) -> (u64, Option<chrono::DateTime<chrono::Local>>) {
    let mut total_size = 0u64;
    let mut max_modified: Option<chrono::DateTime<chrono::Local>> = None;

    for &path in paths {
        let (size, modified) = calculate_dir_size(path);
        total_size = total_size.saturating_add(size);
        if let Some(m) = modified {
            max_modified = Some(match max_modified {
                Some(current) => current.max(m),
                None => m,
            });
        }
    }

    (total_size, max_modified)
}

/// Spawns a background threadpool of workers to process sizing jobs from `rx` and send results to `tx`.
pub fn spawn_sizer_workers(
    rx: Receiver<PathBuf>,
    tx: Sender<SizeResult>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("sizer-dispatcher".to_string())
        .spawn(move || {
            let pool = rayon::ThreadPoolBuilder::new()
                .thread_name(|idx| format!("sizer-worker-{}", idx))
                .build()
                .expect("Failed to initialize sizer rayon threadpool");

            while let Ok(path) = rx.recv() {
                let tx = tx.clone();
                pool.spawn(move || {
                    let (size_bytes, last_modified) = calculate_dir_size(&path);
                    let _ = tx.send(SizeResult {
                        path,
                        size_bytes,
                        last_modified,
                    });
                });
            }

            drop(pool);
            drop(tx);
        })
        .expect("Failed to spawn sizer dispatcher thread")
}

/// Spawns a background threadpool of workers to process `DiscoveredArtifact` jobs from `rx`
/// and stream results to `tx`. Aggregated artifacts with multiple `sub_paths` are sized across all targets.
pub fn spawn_artifact_sizer_workers(
    rx: Receiver<DiscoveredArtifact>,
    tx: Sender<SizeResult>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("sizer-artifact-dispatcher".to_string())
        .spawn(move || {
            let pool = rayon::ThreadPoolBuilder::new()
                .thread_name(|idx| format!("sizer-worker-{}", idx))
                .build()
                .expect("Failed to initialize sizer rayon threadpool");

            while let Ok(artifact) = rx.recv() {
                let tx = tx.clone();
                pool.spawn(move || {
                    let targets = artifact.target_paths();
                    let (size_bytes, last_modified) = calculate_paths_size(&targets);
                    let _ = tx.send(SizeResult {
                        path: artifact.path,
                        size_bytes,
                        last_modified,
                    });
                });
            }

            drop(pool);
            drop(tx);
        })
        .expect("Failed to spawn sizer artifact dispatcher thread")
}
