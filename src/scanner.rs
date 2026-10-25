use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use crossbeam_channel::Sender;

use crate::ecosystem::{detect_ecosystem, validate_dotnet_bin_obj, Ecosystem};

/// Discovered regenerable artifact or build directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredArtifact {
    pub path: PathBuf,
    pub project_name: String,
    pub ecosystem: Ecosystem,
    pub sub_paths: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
struct ProjectInfo {
    name: String,
    ecosystem: Ecosystem,
    root: PathBuf,
}

#[derive(Clone, Debug, Default)]
struct ScanContext {
    current_project: Option<ProjectInfo>,
}

fn emit_artifact(
    path: &Path,
    project_name: String,
    ecosystem: Ecosystem,
    sub_paths: Vec<PathBuf>,
    sender: &Sender<DiscoveredArtifact>,
    seen_paths: &Arc<Mutex<HashSet<PathBuf>>>,
) {
    let norm_path = dunce::simplified(path).to_path_buf();
    let mut seen = seen_paths.lock().unwrap();
    if seen.insert(norm_path.clone()) {
        let _ = sender.send(DiscoveredArtifact {
            path: norm_path,
            project_name,
            ecosystem,
            sub_paths,
        });
    }
}

/// Recursively scans `root` for regenerable build artifacts and dependencies across supported ecosystems,
/// streaming results to the provided `sender`.
pub fn scan_directory(root: &Path, sender: Sender<DiscoveredArtifact>) {
    if !root.exists() {
        return;
    }

    let seen_paths = Arc::new(Mutex::new(HashSet::<PathBuf>::new()));
    // Maps project_root -> (project_name, Vec<pycache_dirs>)
    let pycache_map = Arc::new(Mutex::new(HashMap::<PathBuf, (String, Vec<PathBuf>)>::new()));

    let seen_paths_cloned = Arc::clone(&seen_paths);
    let pycache_map_cloned = Arc::clone(&pycache_map);
    let sender_cloned = sender.clone();

    let walker = jwalk::WalkDirGeneric::<(ScanContext, ())>::new(root)
        .skip_hidden(false)
        .parallelism(jwalk::Parallelism::RayonDefaultPool {
            busy_timeout: std::time::Duration::from_secs(10),
        })
        .process_read_dir(move |_depth, dir_path, state, children| {
            // Guard: Never inspect or descend into .git
            if dir_path.file_name().map(|n| n == ".git").unwrap_or(false) {
                children.clear();
                return;
            }

            // Exclude .git entries completely
            children.retain(|c| {
                if let Ok(entry) = c {
                    entry.file_name != ".git"
                } else {
                    true
                }
            });

            // Extract child file/folder names
            let mut file_names = Vec::with_capacity(children.len());
            for entry in children.iter().flatten() {
                file_names.push(entry.file_name.to_string_lossy().to_string());
            }
            let file_name_strs: Vec<&str> = file_names.iter().map(|s| s.as_str()).collect();

            // Detect if this directory is a project boundary
            if let Some(eco) = detect_ecosystem(&file_name_strs) {
                let project_name = dir_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "root".to_string());
                state.current_project = Some(ProjectInfo {
                    name: project_name,
                    ecosystem: eco,
                    root: dir_path.to_path_buf(),
                });
            }

            // Inspect children for candidate artifact directories
            for entry_res in children.iter_mut() {
                let entry = match entry_res {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                if !entry.file_type.is_dir() {
                    continue;
                }

                let child_name_os = &entry.file_name;
                let child_name = child_name_os.to_string_lossy();
                let child_path = dir_path.join(child_name_os);

                // 1. Rust: target (only if direct child of directory containing Cargo.toml)
                if child_name == "target" {
                    let has_cargo = file_name_strs
                        .iter()
                        .any(|f| f.eq_ignore_ascii_case("Cargo.toml"));
                    if has_cargo {
                        entry.read_children_path = None;
                        let proj_name = dir_path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "root".to_string());
                        emit_artifact(
                            &child_path,
                            proj_name,
                            Ecosystem::Rust,
                            vec![],
                            &sender_cloned,
                            &seen_paths_cloned,
                        );
                        continue;
                    }
                }

                // 2. .NET: bin & obj (strict contextual validation)
                if (child_name.eq_ignore_ascii_case("bin") || child_name.eq_ignore_ascii_case("obj"))
                    && validate_dotnet_bin_obj(&child_path)
                {
                    entry.read_children_path = None;
                    let proj_name = state
                        .current_project
                        .as_ref()
                        .filter(|p| p.ecosystem == Ecosystem::DotNet)
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| {
                            dir_path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "root".to_string())
                        });
                    emit_artifact(
                        &child_path,
                        proj_name,
                        Ecosystem::DotNet,
                        vec![],
                        &sender_cloned,
                        &seen_paths_cloned,
                    );
                    continue;
                }

                // 3. Flutter: .dart_tool, build, ephemeral
                if child_name == ".dart_tool" {
                    let has_pubspec = file_name_strs
                        .iter()
                        .any(|f| f.eq_ignore_ascii_case("pubspec.yaml"));
                    let is_flutter_proj = state
                        .current_project
                        .as_ref()
                        .map(|p| p.ecosystem == Ecosystem::Flutter)
                        .unwrap_or(false);
                    if has_pubspec || is_flutter_proj {
                        entry.read_children_path = None;
                        let proj_name = state
                            .current_project
                            .as_ref()
                            .map(|p| p.name.clone())
                            .unwrap_or_else(|| {
                                dir_path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "root".to_string())
                            });
                        emit_artifact(
                            &child_path,
                            proj_name,
                            Ecosystem::Flutter,
                            vec![],
                            &sender_cloned,
                            &seen_paths_cloned,
                        );
                        continue;
                    }
                }

                if child_name == "ephemeral" {
                    let is_flutter_proj = state
                        .current_project
                        .as_ref()
                        .map(|p| p.ecosystem == Ecosystem::Flutter)
                        .unwrap_or(false);
                    let parent_is_flutter =
                        dir_path.file_name().map(|n| n == "flutter").unwrap_or(false);
                    if is_flutter_proj || parent_is_flutter {
                        entry.read_children_path = None;
                        let proj_name = state
                            .current_project
                            .as_ref()
                            .map(|p| p.name.clone())
                            .unwrap_or_else(|| "flutter_app".to_string());
                        emit_artifact(
                            &child_path,
                            proj_name,
                            Ecosystem::Flutter,
                            vec![],
                            &sender_cloned,
                            &seen_paths_cloned,
                        );
                        continue;
                    }
                }

                // Build folder: distinguish Flutter build vs Node build vs ignore
                if child_name == "build" {
                    let has_pubspec = file_name_strs
                        .iter()
                        .any(|f| f.eq_ignore_ascii_case("pubspec.yaml"));
                    let is_flutter_proj = state
                        .current_project
                        .as_ref()
                        .map(|p| p.ecosystem == Ecosystem::Flutter)
                        .unwrap_or(false);
                    let is_android_app = dir_path.ends_with("android/app")
                        || dir_path.ends_with("android\\app");

                    if has_pubspec || (is_flutter_proj && is_android_app) {
                        entry.read_children_path = None;
                        let proj_name = state
                            .current_project
                            .as_ref()
                            .map(|p| p.name.clone())
                            .unwrap_or_else(|| {
                                dir_path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "root".to_string())
                            });
                        emit_artifact(
                            &child_path,
                            proj_name,
                            Ecosystem::Flutter,
                            vec![],
                            &sender_cloned,
                            &seen_paths_cloned,
                        );
                        continue;
                    }

                    // Check Node build
                    let has_pkg = file_name_strs
                        .iter()
                        .any(|f| f.eq_ignore_ascii_case("package.json"));
                    if has_pkg && !child_path.join("package.json").exists() {
                        entry.read_children_path = None;
                        let proj_name = dir_path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "root".to_string());
                        emit_artifact(
                            &child_path,
                            proj_name,
                            Ecosystem::Node,
                            vec![],
                            &sender_cloned,
                            &seen_paths_cloned,
                        );
                        continue;
                    }
                }

                // 4. Node: node_modules, .next, .turbo, .nuxt, dist
                if child_name == "node_modules"
                    || child_name == ".next"
                    || child_name == ".turbo"
                    || child_name == ".nuxt"
                {
                    let has_pkg = file_name_strs
                        .iter()
                        .any(|f| f.eq_ignore_ascii_case("package.json"));
                    let is_node_proj = state
                        .current_project
                        .as_ref()
                        .map(|p| p.ecosystem == Ecosystem::Node)
                        .unwrap_or(false);

                    if has_pkg || is_node_proj {
                        entry.read_children_path = None;
                        let proj_name = if has_pkg {
                            dir_path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "root".to_string())
                        } else {
                            state
                                .current_project
                                .as_ref()
                                .map(|p| p.name.clone())
                                .unwrap_or_else(|| {
                                    dir_path
                                        .file_name()
                                        .map(|n| n.to_string_lossy().to_string())
                                        .unwrap_or_else(|| "root".to_string())
                                })
                        };
                        emit_artifact(
                            &child_path,
                            proj_name,
                            Ecosystem::Node,
                            vec![],
                            &sender_cloned,
                            &seen_paths_cloned,
                        );
                        continue;
                    }
                }

                if child_name == "dist" {
                    let has_pkg = file_name_strs
                        .iter()
                        .any(|f| f.eq_ignore_ascii_case("package.json"));
                    if has_pkg && !child_path.join("package.json").exists() {
                        entry.read_children_path = None;
                        let proj_name = dir_path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "root".to_string());
                        emit_artifact(
                            &child_path,
                            proj_name,
                            Ecosystem::Node,
                            vec![],
                            &sender_cloned,
                            &seen_paths_cloned,
                        );
                        continue;
                    }
                }

                // 5. Python: __pycache__, .venv, venv, .pytest_cache, .ruff_cache, .mypy_cache
                let is_python_proj = state
                    .current_project
                    .as_ref()
                    .map(|p| p.ecosystem == Ecosystem::Python)
                    .unwrap_or(false);
                let has_py_manifest = file_name_strs.iter().any(|f| {
                    let lower = f.to_ascii_lowercase();
                    lower == "pyproject.toml"
                        || lower == "requirements.txt"
                        || lower == "pipfile"
                        || lower == "setup.py"
                });

                if child_name == "__pycache__" && (is_python_proj || has_py_manifest) {
                    entry.read_children_path = None;
                    let (proj_name, proj_root) = if let Some(proj) = &state.current_project {
                        (proj.name.clone(), proj.root.clone())
                    } else {
                        let name = dir_path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "root".to_string());
                        (name, dir_path.to_path_buf())
                    };

                    let norm_child = dunce::simplified(&child_path).to_path_buf();
                    let mut map = pycache_map_cloned.lock().unwrap();
                    map.entry(proj_root)
                        .or_insert_with(|| (proj_name, Vec::new()))
                        .1
                        .push(norm_child);
                    continue;
                }

                if child_name == ".venv" || child_name == "venv" {
                    let has_pyvenv_cfg = child_path.join("pyvenv.cfg").exists();
                    if is_python_proj || has_py_manifest || has_pyvenv_cfg {
                        entry.read_children_path = None;
                        let proj_name = state
                            .current_project
                            .as_ref()
                            .map(|p| p.name.clone())
                            .unwrap_or_else(|| {
                                dir_path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "root".to_string())
                            });
                        emit_artifact(
                            &child_path,
                            proj_name,
                            Ecosystem::Python,
                            vec![],
                            &sender_cloned,
                            &seen_paths_cloned,
                        );
                        continue;
                    }
                }

                if (child_name == ".pytest_cache"
                    || child_name == ".ruff_cache"
                    || child_name == ".mypy_cache")
                    && (is_python_proj || has_py_manifest)
                {
                    entry.read_children_path = None;
                    let proj_name = state
                        .current_project
                        .as_ref()
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| {
                            dir_path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "root".to_string())
                        });
                    emit_artifact(
                        &child_path,
                        proj_name,
                        Ecosystem::Python,
                        vec![],
                        &sender_cloned,
                        &seen_paths_cloned,
                    );
                    continue;
                }
            }
        });

    // Run directory walk to completion across rayon worker pool
    for _ in walker {}

    // Emit consolidated Python __pycache__ artifacts per project
    let mut map = pycache_map.lock().unwrap();
    for (proj_root, (proj_name, mut sub_paths)) in map.drain() {
        sub_paths.sort();
        sub_paths.dedup();
        let path = dunce::simplified(&proj_root.join("__pycache__")).to_path_buf();
        emit_artifact(
            &path,
            proj_name,
            Ecosystem::Python,
            sub_paths,
            &sender,
            &seen_paths,
        );
    }
}

/// Synchronous scan helper that collects all discovered artifacts into a `Vec`.
pub fn scan_directory_sync(root: &Path) -> Vec<DiscoveredArtifact> {
    let (tx, rx) = crossbeam_channel::unbounded();
    scan_directory(root, tx);
    rx.into_iter().collect()
}
