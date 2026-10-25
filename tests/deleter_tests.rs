use devclean::deleter::{delete_artifact, delete_artifact_path, delete_paths, DeletionResult};
use devclean::ecosystem::Ecosystem;
use devclean::scanner::DiscoveredArtifact;
use std::fs::{create_dir_all, write};
use tempfile::tempdir;

#[test]
fn test_dry_run_does_not_delete() {
    let tmp = tempdir().unwrap();
    let folder = tmp.path().join("node_modules");
    create_dir_all(&folder).unwrap();
    write(folder.join("package.json"), "{}").unwrap();

    let result = delete_artifact_path(&folder, 1024, true);
    assert_eq!(result, DeletionResult::Success { freed_bytes: 1024 });
    assert!(folder.exists(), "Folder must still exist after dry-run");
}

#[test]
fn test_actual_deletion_removes_directory() {
    let tmp = tempdir().unwrap();
    let folder = tmp.path().join("target");
    create_dir_all(&folder).unwrap();
    write(folder.join("binary.exe"), "abc").unwrap();

    let result = delete_artifact_path(&folder, 3, false);
    assert_eq!(result, DeletionResult::Success { freed_bytes: 3 });
    assert!(!folder.exists(), "Folder must be removed");
}

#[test]
fn test_delete_non_existent_path_fails() {
    let tmp = tempdir().unwrap();
    let non_existent = tmp.path().join("does_not_exist");

    // Dry-run should fail if path doesn't exist
    let dry_res = delete_artifact_path(&non_existent, 100, true);
    assert!(
        matches!(dry_res, DeletionResult::Failed { .. }),
        "Dry-run on non-existent path should return Failed"
    );

    // Actual deletion should fail if path doesn't exist
    let actual_res = delete_artifact_path(&non_existent, 100, false);
    assert!(
        matches!(actual_res, DeletionResult::Failed { .. }),
        "Actual deletion on non-existent path should return Failed"
    );
}

#[test]
fn test_delete_paths_composite_artifacts() {
    let tmp = tempdir().unwrap();
    let pycache1 = tmp.path().join("pkg").join("sub1").join("__pycache__");
    let pycache2 = tmp.path().join("pkg").join("sub2").join("__pycache__");
    create_dir_all(&pycache1).unwrap();
    create_dir_all(&pycache2).unwrap();
    write(pycache1.join("a.pyc"), "123").unwrap();
    write(pycache2.join("b.pyc"), "456").unwrap();

    let result = delete_paths(&[&pycache1, &pycache2], 6, false);
    assert_eq!(result, DeletionResult::Success { freed_bytes: 6 });
    assert!(!pycache1.exists());
    assert!(!pycache2.exists());
}

#[test]
fn test_delete_paths_dry_run() {
    let tmp = tempdir().unwrap();
    let d1 = tmp.path().join("dir1");
    let d2 = tmp.path().join("dir2");
    create_dir_all(&d1).unwrap();
    create_dir_all(&d2).unwrap();
    write(d1.join("file.txt"), "hello").unwrap();
    write(d2.join("file.txt"), "world").unwrap();

    let result = delete_paths(&[&d1, &d2], 10, true);
    assert_eq!(result, DeletionResult::Success { freed_bytes: 10 });
    assert!(d1.exists());
    assert!(d2.exists());
}

#[test]
fn test_delete_paths_empty_slice() {
    let result = delete_paths(&[], 0, false);
    assert_eq!(result, DeletionResult::Success { freed_bytes: 0 });
}

#[test]
fn test_delete_readonly_file_inside_dir() {
    let tmp = tempdir().unwrap();
    let folder = tmp.path().join("node_modules");
    create_dir_all(&folder).unwrap();
    let ro_file = folder.join("read_only.txt");
    write(&ro_file, "protected data").unwrap();

    let mut perms = std::fs::metadata(&ro_file).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&ro_file, perms).unwrap();

    let result = delete_artifact_path(&folder, 14, false);
    assert_eq!(result, DeletionResult::Success { freed_bytes: 14 });
    assert!(!folder.exists(), "Folder with readonly file should be deleted");
}

#[test]
fn test_delete_readonly_directory() {
    let tmp = tempdir().unwrap();
    let folder = tmp.path().join("vendor");
    let sub = folder.join("sub");
    create_dir_all(&sub).unwrap();
    write(sub.join("test.txt"), "content").unwrap();

    // Set readonly on sub folder
    let mut perms = std::fs::metadata(&sub).unwrap().permissions();
    perms.set_readonly(true);
    let _ = std::fs::set_permissions(&sub, perms);

    let result = delete_artifact_path(&folder, 7, false);
    assert_eq!(result, DeletionResult::Success { freed_bytes: 7 });
    assert!(!folder.exists(), "Folder with readonly subdirectory should be deleted");
}

#[test]
fn test_delete_single_file() {
    let tmp = tempdir().unwrap();
    let file = tmp.path().join("stray_artifact.log");
    write(&file, "log content").unwrap();

    let result = delete_artifact_path(&file, 11, false);
    assert_eq!(result, DeletionResult::Success { freed_bytes: 11 });
    assert!(!file.exists());
}

#[test]
fn test_delete_path_exceeding_max_path() {
    let tmp = tempdir().unwrap();
    let long_segment = "a_very_long_directory_name_segment_to_exceed_limits";
    let mut current = tmp.path().to_path_buf();
    for _ in 0..7 {
        current = current.join(long_segment);
    }
    create_dir_all(&current).unwrap();
    write(current.join("leaf.txt"), "data").unwrap();

    let root_to_delete = tmp.path().join(long_segment);
    let result = delete_artifact_path(&root_to_delete, 4, false);
    assert_eq!(result, DeletionResult::Success { freed_bytes: 4 });
    assert!(!root_to_delete.exists(), "Directory exceeding MAX_PATH should be deleted");
}

#[test]
fn test_delete_artifact_helper() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("python_project");
    let pycache1 = root.join("pkg1").join("__pycache__");
    let pycache2 = root.join("pkg2").join("__pycache__");
    create_dir_all(&pycache1).unwrap();
    create_dir_all(&pycache2).unwrap();
    write(pycache1.join("a.pyc"), "123").unwrap();
    write(pycache2.join("b.pyc"), "456").unwrap();

    let artifact = DiscoveredArtifact {
        path: root.join("__pycache__"),
        project_name: "python_project".to_string(),
        ecosystem: Ecosystem::Python,
        sub_paths: vec![pycache1.clone(), pycache2.clone()],
    };

    let result = delete_artifact(&artifact, 6, false);
    assert_eq!(result, DeletionResult::Success { freed_bytes: 6 });
    assert!(!pycache1.exists());
    assert!(!pycache2.exists());
}

#[test]
fn test_delete_symlink_does_not_delete_target() {
    let tmp = tempdir().unwrap();
    let real_dir = tmp.path().join("real_data");
    create_dir_all(&real_dir).unwrap();
    let target_file = real_dir.join("vital.txt");
    write(&target_file, "vital content").unwrap();

    let link_dir = tmp.path().join("link_to_real");

    #[cfg(windows)]
    let sym_res = std::os::windows::fs::symlink_dir(&real_dir, &link_dir);
    #[cfg(unix)]
    let sym_res = std::os::unix::fs::symlink(&real_dir, &link_dir);

    if sym_res.is_ok() {
        let result = delete_artifact_path(&link_dir, 13, false);
        assert_eq!(result, DeletionResult::Success { freed_bytes: 13 });
        assert!(!link_dir.exists(), "Symlink itself should be removed");
        assert!(real_dir.exists(), "Target directory must NOT be deleted");
        assert!(target_file.exists(), "Target file must NOT be deleted");
    }
}
