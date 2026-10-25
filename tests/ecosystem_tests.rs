use devclean::ecosystem::{
    detect_ecosystem, is_candidate_name, validate_dotnet_bin_obj, Ecosystem, ARTIFACT_RULES,
};
use std::fs::{create_dir_all, File};
use tempfile::tempdir;

#[test]
fn test_detect_ecosystem_markers() {
    assert_eq!(detect_ecosystem(&["package.json"]), Some(Ecosystem::Node));
    assert_eq!(detect_ecosystem(&["pubspec.yaml"]), Some(Ecosystem::Flutter));
    assert_eq!(detect_ecosystem(&["Cargo.toml"]), Some(Ecosystem::Rust));
    assert_eq!(detect_ecosystem(&["app.csproj"]), Some(Ecosystem::DotNet));
    assert_eq!(detect_ecosystem(&["requirements.txt"]), Some(Ecosystem::Python));
    assert_eq!(detect_ecosystem(&["unknown.txt"]), None);
}

#[test]
fn test_candidate_folder_names() {
    assert_eq!(is_candidate_name("node_modules"), Some(Ecosystem::Node));
    assert_eq!(is_candidate_name(".next"), Some(Ecosystem::Node));
    assert_eq!(is_candidate_name(".dart_tool"), Some(Ecosystem::Flutter));
    assert_eq!(is_candidate_name("target"), Some(Ecosystem::Rust));
    assert_eq!(is_candidate_name("bin"), Some(Ecosystem::DotNet));
    assert_eq!(is_candidate_name("obj"), Some(Ecosystem::DotNet));
    assert_eq!(is_candidate_name("__pycache__"), Some(Ecosystem::Python));
    assert_eq!(is_candidate_name(".venv"), Some(Ecosystem::Python));
    assert_eq!(is_candidate_name("src"), None);
}

#[test]
fn test_validate_dotnet_bin_obj() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // Case 1: Valid .NET project with bin/Debug
    let dotnet_proj = root.join("MyDotNetApp");
    create_dir_all(&dotnet_proj).unwrap();
    File::create(dotnet_proj.join("MyDotNetApp.csproj")).unwrap();
    let bin_dir = dotnet_proj.join("bin");
    create_dir_all(bin_dir.join("Debug")).unwrap();
    assert!(validate_dotnet_bin_obj(&bin_dir));

    // Case 2: Valid .NET obj with net8.0
    let obj_dir = dotnet_proj.join("obj");
    create_dir_all(obj_dir.join("net8.0")).unwrap();
    assert!(validate_dotnet_bin_obj(&obj_dir));

    // Case 3: Node package with bin/cli.js (no csproj/fsproj, no compiler dirs)
    let node_pkg = root.join("my-node-cli");
    create_dir_all(&node_pkg).unwrap();
    File::create(node_pkg.join("package.json")).unwrap();
    let node_bin = node_pkg.join("bin");
    create_dir_all(&node_bin).unwrap();
    File::create(node_bin.join("cli.js")).unwrap();
    assert!(!validate_dotnet_bin_obj(&node_bin));

    // Case 4: Parent has csproj, but bin only contains a plain text file without compiler subfolders
    let empty_bin_proj = root.join("EmptyBinProj");
    create_dir_all(&empty_bin_proj).unwrap();
    File::create(empty_bin_proj.join("Empty.csproj")).unwrap();
    let empty_bin = empty_bin_proj.join("bin");
    create_dir_all(&empty_bin).unwrap();
    File::create(empty_bin.join("something.txt")).unwrap();
    assert!(!validate_dotnet_bin_obj(&empty_bin));
}

#[test]
fn test_artifact_rules_definition() {
    assert!(!ARTIFACT_RULES.is_empty());
    assert!(ARTIFACT_RULES.iter().any(|r| r.target_name == "node_modules" && r.ecosystem == Ecosystem::Node));
    assert!(ARTIFACT_RULES.iter().any(|r| r.target_name == "target" && r.ecosystem == Ecosystem::Rust));
    assert!(ARTIFACT_RULES.iter().any(|r| r.target_name == "bin" && r.ecosystem == Ecosystem::DotNet));
}
