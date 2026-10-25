use std::path::Path;

use crate::scanner::DiscoveredArtifact;

/// Outcome of deleting an artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeletionResult {
    /// Deletion succeeded (or simulated deletion succeeded in dry-run mode).
    Success { freed_bytes: u64 },
    /// Deletion failed due to an I/O or permissions error.
    Failed { error: String },
}

/// Recursively resets the read-only attribute on a directory tree so deletion can proceed on Windows.
#[allow(clippy::permissions_set_readonly_false)]
fn reset_readonly_recursive(path: &Path) {
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.permissions().readonly() {
            let mut perms = meta.permissions();
            perms.set_readonly(false);
            let _ = std::fs::set_permissions(path, perms);
        }
        if meta.is_dir() && !meta.file_type().is_symlink() {
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    reset_readonly_recursive(&entry.path());
                }
            }
        }
    }
}

/// Safely removes a file, resetting the read-only attribute if needed.
#[allow(clippy::permissions_set_readonly_false)]
fn remove_file_resilient(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) => {
            if let Ok(meta) = std::fs::symlink_metadata(path) {
                if meta.permissions().readonly() {
                    let mut perms = meta.permissions();
                    perms.set_readonly(false);
                    let _ = std::fs::set_permissions(path, perms);
                    return std::fs::remove_file(path);
                }
            }
            Err(err)
        }
    }
}

/// Removes a symlink or Windows junction without canonicalizing to or traversing into the target.
#[allow(clippy::permissions_set_readonly_false)]
fn remove_symlink_or_junction(path: &Path, meta: &std::fs::Metadata) -> std::io::Result<()> {
    let res = if meta.is_dir() {
        std::fs::remove_dir(path).or_else(|_| std::fs::remove_file(path))
    } else {
        std::fs::remove_file(path).or_else(|_| std::fs::remove_dir(path))
    };

    if let Err(err) = res {
        if meta.permissions().readonly() {
            let mut perms = meta.permissions();
            perms.set_readonly(false);
            let _ = std::fs::set_permissions(path, perms);
            return if meta.is_dir() {
                std::fs::remove_dir(path).or_else(|_| std::fs::remove_file(path))
            } else {
                std::fs::remove_file(path).or_else(|_| std::fs::remove_dir(path))
            };
        }
        return Err(err);
    }

    Ok(())
}

/// Recursively removes a directory tree, handling Windows read-only flags and transient locks with retries.
fn remove_dir_all_resilient(path: &Path) -> std::io::Result<()> {
    const MAX_ATTEMPTS: usize = 3;
    let mut last_err = None;

    for attempt in 0..MAX_ATTEMPTS {
        match std::fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_err = Some(err);
                // Clear read-only attributes on remaining files/subdirectories
                reset_readonly_recursive(path);
                if attempt + 1 < MAX_ATTEMPTS {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
            }
        }
    }

    Err(last_err.unwrap_or_else(|| {
        std::io::Error::other("Failed to remove directory")
    }))
}

/// Safely deletes a single path, whether it is a directory, file, or symlink.
fn delete_single_path_safely(path: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;

    if meta.file_type().is_symlink() {
        // Symlinks and junctions must not be canonicalized to avoid deleting target contents
        return remove_symlink_or_junction(path, &meta);
    }

    if meta.is_file() {
        return remove_file_resilient(path);
    }

    if meta.is_dir() {
        let canonical = dunce::canonicalize(path)
            .or_else(|_| std::fs::canonicalize(path))
            .unwrap_or_else(|_| path.to_path_buf());
        return remove_dir_all_resilient(&canonical);
    }

    std::fs::remove_file(path)
}

/// Deletes a set of filesystem paths belonging to an artifact.
///
/// In dry-run mode (`dry_run == true`), verifies that all paths exist without modifying disk,
/// returning `DeletionResult::Success { freed_bytes: reported_bytes }` on success.
/// In actual deletion mode (`dry_run == false`), deletes all paths safely and returns
/// `DeletionResult::Success { freed_bytes: reported_bytes }` or `DeletionResult::Failed { error }`.
pub fn delete_paths(paths: &[&Path], reported_bytes: u64, dry_run: bool) -> DeletionResult {
    if paths.is_empty() {
        return DeletionResult::Success {
            freed_bytes: reported_bytes,
        };
    }

    if dry_run {
        for path in paths {
            if std::fs::symlink_metadata(path).is_err() {
                return DeletionResult::Failed {
                    error: format!("Path does not exist: {}", path.display()),
                };
            }
        }
        return DeletionResult::Success {
            freed_bytes: reported_bytes,
        };
    }

    for path in paths {
        if std::fs::symlink_metadata(path).is_err() {
            return DeletionResult::Failed {
                error: format!("Path does not exist: {}", path.display()),
            };
        }

        if let Err(err) = delete_single_path_safely(path) {
            return DeletionResult::Failed {
                error: format!("Failed to delete {}: {}", path.display(), err),
            };
        }
    }

    DeletionResult::Success {
        freed_bytes: reported_bytes,
    }
}

/// Deletes a single artifact directory or file path.
///
/// In dry-run mode (`dry_run == true`), checks existence and returns simulated success.
/// In actual mode (`dry_run == false`), removes the directory/file and returns success or failure.
pub fn delete_artifact_path(path: &Path, reported_bytes: u64, dry_run: bool) -> DeletionResult {
    delete_paths(&[path], reported_bytes, dry_run)
}

/// Convenience helper to delete a `DiscoveredArtifact`, whether single-path or composite.
pub fn delete_artifact(
    artifact: &DiscoveredArtifact,
    reported_bytes: u64,
    dry_run: bool,
) -> DeletionResult {
    delete_paths(&artifact.target_paths(), reported_bytes, dry_run)
}
