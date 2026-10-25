use std::fs::{create_dir_all, write};
use tempfile::tempdir;

use devclean::sizer::{
    calculate_dir_size, calculate_paths_size, spawn_artifact_sizer_workers, spawn_sizer_workers,
    SizeResult,
};
use devclean::scanner::DiscoveredArtifact;
use devclean::ecosystem::Ecosystem;

#[test]
fn test_calculate_dir_size_with_files() {
    let tmp = tempdir().unwrap();
    let dir = tmp.path().join("test_artifact");
    create_dir_all(dir.join("sub")).unwrap();
    write(dir.join("file1.txt"), "hello world").unwrap(); // 11 bytes
    write(dir.join("sub").join("file2.txt"), "1234567890").unwrap(); // 10 bytes

    let (bytes, modified) = calculate_dir_size(&dir);
    assert_eq!(bytes, 21);
    assert!(modified.is_some());
}

#[test]
fn test_calculate_dir_size_empty_dir() {
    let tmp = tempdir().unwrap();
    let dir = tmp.path().join("empty_folder");
    create_dir_all(&dir).unwrap();

    let (bytes, modified) = calculate_dir_size(&dir);
    assert_eq!(bytes, 0);
    assert!(modified.is_some());
}

#[test]
fn test_calculate_dir_size_non_existent() {
    let tmp = tempdir().unwrap();
    let non_existent = tmp.path().join("does_not_exist_at_all");

    let (bytes, modified) = calculate_dir_size(&non_existent);
    assert_eq!(bytes, 0);
    assert!(modified.is_none());
}

#[test]
fn test_calculate_dir_size_single_file() {
    let tmp = tempdir().unwrap();
    let file = tmp.path().join("single_file.bin");
    write(&file, "binary payload").unwrap(); // 14 bytes

    let (bytes, modified) = calculate_dir_size(&file);
    assert_eq!(bytes, 14);
    assert!(modified.is_some());
}

#[test]
fn test_calculate_paths_size_empty() {
    let (bytes, modified) = calculate_paths_size(&[]);
    assert_eq!(bytes, 0);
    assert!(modified.is_none());
}

#[test]
fn test_calculate_paths_size_multiple_paths() {
    let tmp = tempdir().unwrap();
    let dir1 = tmp.path().join("dir1");
    let dir2 = tmp.path().join("dir2");
    create_dir_all(&dir1).unwrap();
    create_dir_all(&dir2).unwrap();

    write(dir1.join("a.txt"), "12345").unwrap(); // 5 bytes
    write(dir2.join("b.txt"), "1234567").unwrap(); // 7 bytes

    let targets = vec![dir1.as_path(), dir2.as_path()];
    let (bytes, modified) = calculate_paths_size(&targets);
    assert_eq!(bytes, 12);
    assert!(modified.is_some());
}

#[test]
fn test_spawn_sizer_workers_streaming() {
    let tmp = tempdir().unwrap();
    let dir1 = tmp.path().join("target1");
    let dir2 = tmp.path().join("target2");
    create_dir_all(&dir1).unwrap();
    create_dir_all(&dir2).unwrap();

    write(dir1.join("build.log"), "first").unwrap(); // 5 bytes
    write(dir2.join("output.bin"), "second output").unwrap(); // 13 bytes

    let (task_tx, task_rx) = crossbeam_channel::unbounded();
    let (res_tx, res_rx) = crossbeam_channel::unbounded();

    let _handle = spawn_sizer_workers(task_rx, res_tx);

    task_tx.send(dir1.clone()).unwrap();
    task_tx.send(dir2.clone()).unwrap();
    drop(task_tx); // Signals no more tasks

    let mut results: Vec<SizeResult> = res_rx.into_iter().collect();
    assert_eq!(results.len(), 2);

    results.sort_by(|a, b| a.path.cmp(&b.path));
    let expected_dir1 = dir1;
    let expected_dir2 = dir2;

    let res1 = results.iter().find(|r| r.path == expected_dir1).unwrap();
    assert_eq!(res1.size_bytes, 5);
    assert!(res1.last_modified.is_some());

    let res2 = results.iter().find(|r| r.path == expected_dir2).unwrap();
    assert_eq!(res2.size_bytes, 13);
    assert!(res2.last_modified.is_some());
}

#[test]
fn test_spawn_artifact_sizer_workers_aggregated() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("python_pkg");
    let pycache1 = root.join("src").join("__pycache__");
    let pycache2 = root.join("tests").join("__pycache__");
    create_dir_all(&pycache1).unwrap();
    create_dir_all(&pycache2).unwrap();

    write(pycache1.join("mod.cpython-311.pyc"), "12345678").unwrap(); // 8 bytes
    write(pycache2.join("test.cpython-311.pyc"), "1234").unwrap(); // 4 bytes

    let artifact = DiscoveredArtifact {
        path: root.join("__pycache__"),
        project_name: "python_pkg".to_string(),
        ecosystem: Ecosystem::Python,
        sub_paths: vec![pycache1, pycache2],
    };

    let (task_tx, task_rx) = crossbeam_channel::unbounded();
    let (res_tx, res_rx) = crossbeam_channel::unbounded();

    let _handle = spawn_artifact_sizer_workers(task_rx, res_tx);

    task_tx.send(artifact.clone()).unwrap();
    drop(task_tx);

    let results: Vec<SizeResult> = res_rx.into_iter().collect();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].path, artifact.path);
    assert_eq!(results[0].size_bytes, 12);
    assert!(results[0].last_modified.is_some());
}

#[test]
fn test_calculate_dir_size_deep_hierarchy() {
    let tmp = tempdir().unwrap();
    let mut deep_dir = tmp.path().to_path_buf();
    for i in 0..15 {
        deep_dir = deep_dir.join(format!("level_{}", i));
    }
    create_dir_all(&deep_dir).unwrap();
    write(deep_dir.join("leaf.txt"), "deep contents").unwrap(); // 13 bytes

    let (bytes, modified) = calculate_dir_size(tmp.path());
    assert_eq!(bytes, 13);
    assert!(modified.is_some());
}

#[test]
fn test_calculate_dir_size_path_exceeding_max_path() {
    let tmp = tempdir().unwrap();
    // Build a directory path longer than Windows MAX_PATH (260 characters)
    let long_segment = "a_very_long_directory_name_segment_to_exceed_limits";
    let mut current = tmp.path().to_path_buf();
    for _ in 0..7 {
        current = current.join(long_segment);
    }
    create_dir_all(&current).unwrap();
    write(current.join("deep_file.txt"), "long path test").unwrap(); // 14 bytes

    let (bytes, modified) = calculate_dir_size(tmp.path());
    assert_eq!(bytes, 14);
    assert!(modified.is_some());
}

#[test]
fn test_spawn_sizer_workers_empty_channel() {
    let (task_tx, task_rx) = crossbeam_channel::unbounded();
    let (res_tx, res_rx) = crossbeam_channel::unbounded();

    let handle = spawn_sizer_workers(task_rx, res_tx);
    drop(task_tx);

    let results: Vec<SizeResult> = res_rx.into_iter().collect();
    assert!(results.is_empty());
    assert!(handle.join().is_ok());
}

#[test]
fn test_calculate_dir_size_symlink_not_traversed() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("root");
    let outside = tmp.path().join("outside");
    create_dir_all(&root).unwrap();
    create_dir_all(&outside).unwrap();

    write(root.join("local.txt"), "123").unwrap(); // 3 bytes
    write(outside.join("external.txt"), "123456789012345").unwrap(); // 15 bytes

    // Attempt to create symlink (skip if OS permissions deny symlink creation)
    #[cfg(windows)]
    let symlink_res = std::os::windows::fs::symlink_dir(&outside, root.join("link_to_outside"));
    #[cfg(unix)]
    let symlink_res = std::os::unix::fs::symlink(&outside, root.join("link_to_outside"));

    if symlink_res.is_ok() {
        let (bytes, _) = calculate_dir_size(&root);
        // Only local.txt should be counted, external.txt via symlink should NOT be traversed
        assert_eq!(bytes, 3);
    }
}

