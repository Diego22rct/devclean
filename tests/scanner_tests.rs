use devclean::ecosystem::Ecosystem;
use devclean::scanner::{scan_directory, scan_directory_sync, DiscoveredArtifact};
use std::fs::{create_dir_all, File};
use tempfile::tempdir;

#[test]
fn test_scanner_discovers_node_modules_and_rust_target() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // Setup Node project
    let node_proj = root.join("my-web-app");
    create_dir_all(&node_proj.join("node_modules")).unwrap();
    create_dir_all(&node_proj.join(".next")).unwrap();
    File::create(node_proj.join("package.json")).unwrap();

    // Setup Rust project
    let rust_proj = root.join("my-rust-crate");
    create_dir_all(&rust_proj.join("target")).unwrap();
    File::create(rust_proj.join("Cargo.toml")).unwrap();

    let artifacts = scan_directory_sync(root);

    assert_eq!(artifacts.len(), 3);
    assert!(artifacts.iter().any(|a| a.path.ends_with("node_modules") && a.ecosystem == Ecosystem::Node));
    assert!(artifacts.iter().any(|a| a.path.ends_with(".next") && a.ecosystem == Ecosystem::Node));
    assert!(artifacts.iter().any(|a| a.path.ends_with("target") && a.ecosystem == Ecosystem::Rust));
    assert!(artifacts.iter().all(|a| !a.project_name.is_empty()));
}

#[test]
fn test_scanner_never_descends_or_flags_git() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let proj = root.join("git-project");
    create_dir_all(&proj).unwrap();
    File::create(proj.join("package.json")).unwrap();

    // Setup .git directory with fake node_modules inside
    let git_dir = proj.join(".git");
    create_dir_all(git_dir.join("node_modules")).unwrap();
    create_dir_all(git_dir.join("objects")).unwrap();

    // Setup mixed-case .Git directory with fake target inside
    let git_dir_mixed = proj.join(".Git");
    create_dir_all(git_dir_mixed.join("target")).unwrap();

    // Setup uppercase .GIT directory with fake bin inside
    let git_dir_upper = proj.join(".GIT");
    create_dir_all(git_dir_upper.join("bin")).unwrap();

    // Normal artifact
    create_dir_all(proj.join("node_modules")).unwrap();

    let artifacts = scan_directory_sync(root);

    assert_eq!(artifacts.len(), 1);
    assert!(artifacts[0].path.ends_with("node_modules"));
    assert!(!artifacts[0].path.to_string_lossy().to_lowercase().contains(".git"));
}

#[test]
fn test_scanner_does_not_descend_into_artifact_folders() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let node_proj = root.join("my-app");
    let node_modules = node_proj.join("node_modules");
    // Nested package inside node_modules that has another node_modules or target
    let nested_pkg = node_modules.join("nested-pkg");
    create_dir_all(nested_pkg.join("node_modules")).unwrap();
    create_dir_all(nested_pkg.join("target")).unwrap();
    File::create(nested_pkg.join("package.json")).unwrap();
    File::create(nested_pkg.join("Cargo.toml")).unwrap();
    File::create(node_proj.join("package.json")).unwrap();

    let artifacts = scan_directory_sync(root);

    // Should only discover top-level node_modules, not descend into it
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].project_name, "my-app");
    assert!(artifacts[0].path.ends_with("node_modules"));
}

#[test]
fn test_scanner_aggregates_python_pycache() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let py_proj = root.join("python-service");
    create_dir_all(py_proj.join("src").join("__pycache__")).unwrap();
    create_dir_all(py_proj.join("tests").join("__pycache__")).unwrap();
    create_dir_all(py_proj.join(".venv")).unwrap();
    File::create(py_proj.join("pyproject.toml")).unwrap();

    let artifacts = scan_directory_sync(root);

    // Should have 1 aggregated __pycache__ and 1 .venv
    assert_eq!(artifacts.len(), 2);

    let pycache_art = artifacts.iter().find(|a| a.path.ends_with("__pycache__")).expect("Aggregated pycache must exist");
    assert_eq!(pycache_art.ecosystem, Ecosystem::Python);
    assert_eq!(pycache_art.project_name, "python-service");
    assert_eq!(pycache_art.sub_paths.len(), 2);
    assert!(pycache_art.sub_paths.iter().all(|p| p.ends_with("__pycache__")));

    let venv_art = artifacts.iter().find(|a| a.path.ends_with(".venv")).expect(".venv must exist");
    assert_eq!(venv_art.ecosystem, Ecosystem::Python);
    assert_eq!(venv_art.project_name, "python-service");
    assert!(venv_art.sub_paths.is_empty());
}

#[test]
fn test_scanner_dotnet_contextual_validation() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // Valid .NET project
    let dotnet_proj = root.join("MyDotNetApp");
    create_dir_all(dotnet_proj.join("bin").join("Debug")).unwrap();
    create_dir_all(dotnet_proj.join("obj").join("Release")).unwrap();
    File::create(dotnet_proj.join("MyDotNetApp.csproj")).unwrap();

    // Node CLI package with bin/cli.js (not .NET compiler output!)
    let node_cli = root.join("my-cli-tool");
    let cli_bin = node_cli.join("bin");
    create_dir_all(&cli_bin).unwrap();
    File::create(cli_bin.join("cli.js")).unwrap();
    File::create(node_cli.join("package.json")).unwrap();

    let artifacts = scan_directory_sync(root);

    assert_eq!(artifacts.len(), 2);
    assert!(artifacts.iter().any(|a| a.path.ends_with("bin") && a.ecosystem == Ecosystem::DotNet));
    assert!(artifacts.iter().any(|a| a.path.ends_with("obj") && a.ecosystem == Ecosystem::DotNet));
    // The node_cli/bin folder must NOT be flagged as an artifact!
    assert!(!artifacts.iter().any(|a| a.project_name == "my-cli-tool"));
}

#[test]
fn test_scanner_ignores_unrelated_build_and_bin_folders() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // Random non-project folder with build and bin folders
    let random_dir = root.join("personal-documents");
    create_dir_all(random_dir.join("build")).unwrap();
    create_dir_all(random_dir.join("bin")).unwrap();
    create_dir_all(random_dir.join("target")).unwrap();
    create_dir_all(random_dir.join("dist")).unwrap();

    let artifacts = scan_directory_sync(root);

    // None should be flagged because there are no project manifests!
    assert!(artifacts.is_empty());
}

#[test]
fn test_scanner_flutter_artifacts() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let flutter_proj = root.join("flutter_gallery");
    create_dir_all(flutter_proj.join(".dart_tool")).unwrap();
    create_dir_all(flutter_proj.join("build")).unwrap();
    create_dir_all(flutter_proj.join("windows").join("flutter").join("ephemeral")).unwrap();
    File::create(flutter_proj.join("pubspec.yaml")).unwrap();

    let artifacts = scan_directory_sync(root);

    assert_eq!(artifacts.len(), 3);
    assert!(artifacts.iter().all(|a| a.ecosystem == Ecosystem::Flutter));
    assert!(artifacts.iter().all(|a| a.project_name == "flutter_gallery"));
    assert!(artifacts.iter().any(|a| a.path.ends_with(".dart_tool")));
    assert!(artifacts.iter().any(|a| a.path.ends_with("build")));
    assert!(artifacts.iter().any(|a| a.path.ends_with("ephemeral")));
}

#[test]
fn test_scanner_streaming_channel() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let rust_proj = root.join("my-lib");
    create_dir_all(rust_proj.join("target")).unwrap();
    File::create(rust_proj.join("Cargo.toml")).unwrap();

    let (tx, rx) = crossbeam_channel::unbounded();
    scan_directory(root, tx);

    let collected: Vec<DiscoveredArtifact> = rx.into_iter().collect();
    assert_eq!(collected.len(), 1);
    assert_eq!(collected[0].project_name, "my-lib");
    assert_eq!(collected[0].ecosystem, Ecosystem::Rust);
}

#[test]
fn test_scanner_non_existent_root_does_not_panic() {
    let non_existent = std::path::PathBuf::from("C:/devclean_fake_non_existent_test_path_12345");
    let artifacts = scan_directory_sync(&non_existent);
    assert!(artifacts.is_empty());
}

#[test]
fn test_scanner_root_is_itself_project() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    create_dir_all(root.join("target")).unwrap();
    File::create(root.join("Cargo.toml")).unwrap();

    let artifacts = scan_directory_sync(root);
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].ecosystem, Ecosystem::Rust);
    assert!(artifacts[0].path.ends_with("target"));
}

#[test]
fn test_scanner_monorepo_nested_projects() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // Monorepo root is Node
    let monorepo = root.join("my-monorepo");
    create_dir_all(monorepo.join("node_modules")).unwrap();
    File::create(monorepo.join("package.json")).unwrap();

    // Nested Rust backend
    let backend = monorepo.join("packages").join("backend");
    create_dir_all(backend.join("target")).unwrap();
    File::create(backend.join("Cargo.toml")).unwrap();

    // Nested Python service
    let py_svc = monorepo.join("packages").join("ai-service");
    create_dir_all(py_svc.join("src").join("__pycache__")).unwrap();
    create_dir_all(py_svc.join("tests").join("__pycache__")).unwrap();
    File::create(py_svc.join("pyproject.toml")).unwrap();

    let artifacts = scan_directory_sync(root);

    assert_eq!(artifacts.len(), 3);

    let node_art = artifacts.iter().find(|a| a.ecosystem == Ecosystem::Node).unwrap();
    assert_eq!(node_art.project_name, "my-monorepo");
    assert!(node_art.path.ends_with("node_modules"));

    let rust_art = artifacts.iter().find(|a| a.ecosystem == Ecosystem::Rust).unwrap();
    assert_eq!(rust_art.project_name, "backend");
    assert!(rust_art.path.ends_with("target"));

    let py_art = artifacts.iter().find(|a| a.ecosystem == Ecosystem::Python).unwrap();
    assert_eq!(py_art.project_name, "ai-service");
    assert_eq!(py_art.sub_paths.len(), 2);
}

#[test]
fn test_scanner_multiple_python_projects_aggregated_separately() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let py_a = root.join("service-alpha");
    create_dir_all(py_a.join("pkg1").join("__pycache__")).unwrap();
    create_dir_all(py_a.join("pkg2").join("__pycache__")).unwrap();
    File::create(py_a.join("requirements.txt")).unwrap();

    let py_b = root.join("service-beta");
    create_dir_all(py_b.join("__pycache__")).unwrap();
    File::create(py_b.join("setup.py")).unwrap();

    let artifacts = scan_directory_sync(root);

    assert_eq!(artifacts.len(), 2);

    let alpha = artifacts.iter().find(|a| a.project_name == "service-alpha").unwrap();
    assert_eq!(alpha.ecosystem, Ecosystem::Python);
    assert_eq!(alpha.sub_paths.len(), 2);

    let beta = artifacts.iter().find(|a| a.project_name == "service-beta").unwrap();
    assert_eq!(beta.ecosystem, Ecosystem::Python);
    assert_eq!(beta.sub_paths.len(), 1);
}

#[test]
fn test_scanner_flutter_ephemeral_restrictions() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // Flutter project with valid ephemeral under windows/flutter
    // and invalid ephemeral under lib/ephemeral
    let flutter_proj = root.join("flutter_app");
    create_dir_all(flutter_proj.join("windows").join("flutter").join("ephemeral")).unwrap();
    create_dir_all(flutter_proj.join("lib").join("ephemeral")).unwrap();
    File::create(flutter_proj.join("pubspec.yaml")).unwrap();

    // Non-flutter folder containing flutter/ephemeral
    let random_dir = root.join("random_docs");
    create_dir_all(random_dir.join("flutter").join("ephemeral")).unwrap();

    let artifacts = scan_directory_sync(root);

    // Only the valid windows/flutter/ephemeral inside flutter_proj should be flagged
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].ecosystem, Ecosystem::Flutter);
    assert_eq!(artifacts[0].project_name, "flutter_app");
    assert!(artifacts[0].path.ends_with("ephemeral"));
    assert!(artifacts[0].path.to_string_lossy().contains("windows"));
}

#[test]
fn test_discovered_artifact_target_paths() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // Standard artifact with empty sub_paths
    let standard = DiscoveredArtifact {
        path: root.join("node_modules"),
        project_name: "test".to_string(),
        ecosystem: Ecosystem::Node,
        sub_paths: vec![],
    };
    let targets = standard.target_paths();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0], standard.path.as_path());

    // Aggregated artifact with multiple sub_paths
    let sub1 = root.join("pkg1").join("__pycache__");
    let sub2 = root.join("pkg2").join("__pycache__");
    let aggregated = DiscoveredArtifact {
        path: root.join("__pycache__"),
        project_name: "test_py".to_string(),
        ecosystem: Ecosystem::Python,
        sub_paths: vec![sub1.clone(), sub2.clone()],
    };
    let agg_targets = aggregated.target_paths();
    assert_eq!(agg_targets.len(), 2);
    assert_eq!(agg_targets[0], sub1.as_path());
    assert_eq!(agg_targets[1], sub2.as_path());
}

