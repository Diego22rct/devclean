use std::fs::{create_dir_all, write};
use std::path::PathBuf;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use devclean::app::{format_bytes, App, ModalState, SortMode};
use devclean::ecosystem::Ecosystem;
use devclean::scanner::DiscoveredArtifact;
use devclean::sizer::SizeResult;
use tempfile::tempdir;

#[test]
fn test_app_navigation_and_selection() {
    let mut app = App::new(PathBuf::from("."), false);
    app.add_discovered(DiscoveredArtifact {
        path: PathBuf::from("proj1/node_modules"),
        project_name: "proj1".to_string(),
        ecosystem: Ecosystem::Node,
        sub_paths: vec![],
    });
    app.add_discovered(DiscoveredArtifact {
        path: PathBuf::from("proj2/target"),
        project_name: "proj2".to_string(),
        ecosystem: Ecosystem::Rust,
        sub_paths: vec![],
    });

    assert_eq!(app.items.len(), 2);
    assert_eq!(app.cursor, 0);

    app.toggle_select();
    assert!(app.items[0].selected);
    assert!(!app.items[1].selected);

    app.navigate_down();
    assert_eq!(app.cursor, 1);

    app.select_all();
    assert!(app.items[0].selected && app.items[1].selected);

    // Toggle select_all again deselects all
    app.select_all();
    assert!(!app.items[0].selected && !app.items[1].selected);
}

#[test]
fn test_cursor_navigation_bounds() {
    let mut app = App::new(PathBuf::from("."), false);
    assert_eq!(app.cursor, 0);

    // Empty app should not panic or underflow
    app.navigate_up();
    assert_eq!(app.cursor, 0);
    app.navigate_down();
    assert_eq!(app.cursor, 0);

    // Add items
    app.add_discovered(DiscoveredArtifact {
        path: PathBuf::from("a"),
        project_name: "a".to_string(),
        ecosystem: Ecosystem::Node,
        sub_paths: vec![],
    });
    app.add_discovered(DiscoveredArtifact {
        path: PathBuf::from("b"),
        project_name: "b".to_string(),
        ecosystem: Ecosystem::Rust,
        sub_paths: vec![],
    });

    // Top bound
    app.navigate_up();
    assert_eq!(app.cursor, 0);

    // Move to 1
    app.navigate_down();
    assert_eq!(app.cursor, 1);

    // Bottom bound (len == 2, max index == 1)
    app.navigate_down();
    assert_eq!(app.cursor, 1);

    // Back to 0
    app.navigate_up();
    assert_eq!(app.cursor, 0);
}

#[test]
fn test_add_discovered_and_update_size() {
    let mut app = App::new(PathBuf::from("."), false);
    let p = PathBuf::from("proj/node_modules");

    app.add_discovered(DiscoveredArtifact {
        path: p.clone(),
        project_name: "proj".to_string(),
        ecosystem: Ecosystem::Node,
        sub_paths: vec![],
    });

    // Duplicate add should be ignored
    app.add_discovered(DiscoveredArtifact {
        path: p.clone(),
        project_name: "proj".to_string(),
        ecosystem: Ecosystem::Node,
        sub_paths: vec![],
    });
    assert_eq!(app.items.len(), 1);
    assert_eq!(app.items[0].size_bytes, None);
    assert_eq!(app.items[0].last_modified, None);

    let now = chrono::Local::now();
    app.update_size(SizeResult {
        path: p.clone(),
        size_bytes: 4096,
        last_modified: Some(now),
    });

    assert_eq!(app.items[0].size_bytes, Some(4096));
    assert_eq!(app.items[0].last_modified, Some(now));
}

#[test]
fn test_sorting_modes() {
    let mut app = App::new(PathBuf::from("."), false);
    let now = chrono::Local::now();
    let earlier = now - chrono::Duration::hours(2);

    app.add_discovered(DiscoveredArtifact {
        path: PathBuf::from("c_rust/target"),
        project_name: "c_rust".to_string(),
        ecosystem: Ecosystem::Rust,
        sub_paths: vec![],
    });
    app.add_discovered(DiscoveredArtifact {
        path: PathBuf::from("a_node/node_modules"),
        project_name: "a_node".to_string(),
        ecosystem: Ecosystem::Node,
        sub_paths: vec![],
    });
    app.add_discovered(DiscoveredArtifact {
        path: PathBuf::from("b_dotnet/bin"),
        project_name: "b_dotnet".to_string(),
        ecosystem: Ecosystem::DotNet,
        sub_paths: vec![],
    });

    // Update sizes & timestamps
    app.update_size(SizeResult {
        path: PathBuf::from("c_rust/target"),
        size_bytes: 1000,
        last_modified: Some(now),
    });
    app.update_size(SizeResult {
        path: PathBuf::from("a_node/node_modules"),
        size_bytes: 5000,
        last_modified: Some(earlier),
    });
    // b_dotnet remains size None

    // Default mode is SizeDesc
    assert_eq!(app.sort_mode, SortMode::SizeDesc);
    app.sort_items();
    assert_eq!(app.items[0].project_name, "a_node"); // 5000 bytes
    assert_eq!(app.items[1].project_name, "c_rust"); // 1000 bytes
    assert_eq!(app.items[2].project_name, "b_dotnet"); // None at bottom

    // Cycle to DateOldestFirst
    app.cycle_sort();
    assert_eq!(app.sort_mode, SortMode::DateOldestFirst);
    assert_eq!(app.items[0].project_name, "a_node"); // earlier date
    assert_eq!(app.items[1].project_name, "c_rust"); // now date
    assert_eq!(app.items[2].project_name, "b_dotnet"); // None at bottom

    // Cycle to PathAsc
    app.cycle_sort();
    assert_eq!(app.sort_mode, SortMode::PathAsc);
    assert_eq!(app.items[0].project_name, "a_node");
    assert_eq!(app.items[1].project_name, "b_dotnet");
    assert_eq!(app.items[2].project_name, "c_rust");

    // Cycle to EcosystemAsc (.NET, Node, Rust)
    app.cycle_sort();
    assert_eq!(app.sort_mode, SortMode::EcosystemAsc);
    assert_eq!(app.items[0].ecosystem, Ecosystem::DotNet);
    assert_eq!(app.items[1].ecosystem, Ecosystem::Node);
    assert_eq!(app.items[2].ecosystem, Ecosystem::Rust);

    // Cycle back to SizeDesc
    app.cycle_sort();
    assert_eq!(app.sort_mode, SortMode::SizeDesc);
}

#[test]
fn test_cursor_tracks_item_after_sorting() {
    let mut app = App::new(PathBuf::from("."), false);
    app.add_discovered(DiscoveredArtifact {
        path: PathBuf::from("z_small"),
        project_name: "z_small".to_string(),
        ecosystem: Ecosystem::Node,
        sub_paths: vec![],
    });
    app.add_discovered(DiscoveredArtifact {
        path: PathBuf::from("a_large"),
        project_name: "a_large".to_string(),
        ecosystem: Ecosystem::Rust,
        sub_paths: vec![],
    });

    app.update_size(SizeResult {
        path: PathBuf::from("z_small"),
        size_bytes: 10,
        last_modified: None,
    });
    app.update_size(SizeResult {
        path: PathBuf::from("a_large"),
        size_bytes: 1000,
        last_modified: None,
    });

    // Cursor is initially on index 0 ("z_small")
    assert_eq!(app.cursor, 0);
    assert_eq!(app.items[app.cursor].project_name, "z_small");

    // Sort by SizeDesc ("a_large" should become index 0, "z_small" index 1)
    app.sort_items();
    // Cursor should have followed "z_small" to index 1!
    assert_eq!(app.cursor, 1);
    assert_eq!(app.items[app.cursor].project_name, "z_small");
}

#[test]
fn test_modal_state_transitions() {
    let mut app = App::new(PathBuf::from("."), false);

    // With no items selected, opening delete modal shows a Message modal
    app.open_delete_modal();
    assert!(matches!(app.modal, ModalState::Message { .. }));

    app.close_modal();
    assert_eq!(app.modal, ModalState::None);

    // Add item and select it
    app.add_discovered(DiscoveredArtifact {
        path: PathBuf::from("proj/node_modules"),
        project_name: "proj".to_string(),
        ecosystem: Ecosystem::Node,
        sub_paths: vec![],
    });
    app.update_size(SizeResult {
        path: PathBuf::from("proj/node_modules"),
        size_bytes: 2048,
        last_modified: None,
    });

    app.toggle_select();
    assert!(app.items[0].selected);

    app.open_delete_modal();
    assert_eq!(
        app.modal,
        ModalState::ConfirmDelete {
            count: 1,
            total_bytes: 2048,
        }
    );

    app.close_modal();
    assert_eq!(app.modal, ModalState::None);
}

#[test]
fn test_dry_run_deletion_simulation() {
    let tmp = tempdir().unwrap();
    let folder = tmp.path().join("node_modules");
    create_dir_all(&folder).unwrap();
    write(folder.join("package.json"), "{}").unwrap();

    let mut app = App::new(PathBuf::from("."), true); // dry_run = true
    app.add_discovered(DiscoveredArtifact {
        path: folder.clone(),
        project_name: "test".to_string(),
        ecosystem: Ecosystem::Node,
        sub_paths: vec![],
    });
    app.update_size(SizeResult {
        path: folder.clone(),
        size_bytes: 1024,
        last_modified: None,
    });

    // Select item
    app.toggle_select();
    app.open_delete_modal();

    // Confirm deletion
    app.confirm_delete_and_execute();

    // 1. Filesystem MUST NOT be touched
    assert!(folder.exists(), "Dry-run must not delete directory from disk");

    // 2. total_freed_bytes updated
    assert_eq!(app.total_freed_bytes, 1024);

    // 3. Item removed from UI items list
    assert_eq!(app.items.len(), 0);
    assert_eq!(app.cursor, 0);

    // 4. Modal shows simulation message
    if let ModalState::Message { title, text } = &app.modal {
        assert!(title.contains("Dry Run") || title.contains("Simulation"));
        assert!(text.contains("[Simulation] Would have freed"));
    } else {
        panic!("Expected ModalState::Message with simulation info, got {:?}", app.modal);
    }
}

#[test]
fn test_actual_deletion() {
    let tmp = tempdir().unwrap();
    let folder1 = tmp.path().join("target");
    let folder2 = tmp.path().join(".venv");
    create_dir_all(&folder1).unwrap();
    create_dir_all(&folder2).unwrap();
    write(folder1.join("build.log"), "abc").unwrap();
    write(folder2.join("pyvenv.cfg"), "xyz").unwrap();

    let mut app = App::new(PathBuf::from("."), false); // dry_run = false
    app.add_discovered(DiscoveredArtifact {
        path: folder1.clone(),
        project_name: "rust_app".to_string(),
        ecosystem: Ecosystem::Rust,
        sub_paths: vec![],
    });
    app.add_discovered(DiscoveredArtifact {
        path: folder2.clone(),
        project_name: "py_app".to_string(),
        ecosystem: Ecosystem::Python,
        sub_paths: vec![],
    });

    app.update_size(SizeResult {
        path: folder1.clone(),
        size_bytes: 500,
        last_modified: None,
    });
    app.update_size(SizeResult {
        path: folder2.clone(),
        size_bytes: 700,
        last_modified: None,
    });

    // Select only folder1
    app.toggle_select();
    assert!(app.items[0].selected);
    assert!(!app.items[1].selected);

    app.open_delete_modal();
    app.confirm_delete_and_execute();

    // folder1 deleted from disk
    assert!(!folder1.exists(), "folder1 must be deleted");
    // folder2 must still exist
    assert!(folder2.exists(), "folder2 must not be deleted");

    // Total freed
    assert_eq!(app.total_freed_bytes, 500);

    // items list now only has folder2
    assert_eq!(app.items.len(), 1);
    assert_eq!(app.items[0].path, folder2);
    assert_eq!(app.cursor, 0);

    // Modal shows success message
    if let ModalState::Message { title, text } = &app.modal {
        assert!(title.contains("Completed") || title.contains("Success"));
        assert!(text.contains("Successfully freed"));
    } else {
        panic!("Expected ModalState::Message, got {:?}", app.modal);
    }
}

#[test]
fn test_deletion_composite_pycache() {
    let tmp = tempdir().unwrap();
    let pycache1 = tmp.path().join("mod1").join("__pycache__");
    let pycache2 = tmp.path().join("mod2").join("__pycache__");
    create_dir_all(&pycache1).unwrap();
    create_dir_all(&pycache2).unwrap();
    write(pycache1.join("mod1.pyc"), "123").unwrap();
    write(pycache2.join("mod2.pyc"), "456").unwrap();

    let rep_path = tmp.path().join("__pycache__");
    let mut app = App::new(PathBuf::from("."), false);
    app.add_discovered(DiscoveredArtifact {
        path: rep_path.clone(),
        project_name: "python_pkg".to_string(),
        ecosystem: Ecosystem::Python,
        sub_paths: vec![pycache1.clone(), pycache2.clone()],
    });

    app.update_size(SizeResult {
        path: rep_path.clone(),
        size_bytes: 6,
        last_modified: None,
    });

    app.toggle_select();
    app.open_delete_modal();
    app.confirm_delete_and_execute();

    // Both sub_paths should be deleted from disk
    assert!(!pycache1.exists());
    assert!(!pycache2.exists());
    assert_eq!(app.total_freed_bytes, 6);
    assert_eq!(app.items.len(), 0);
}

#[test]
fn test_rapid_keypress_resilience_and_locking() {
    let mut app = App::new(PathBuf::from("."), false);
    app.add_discovered(DiscoveredArtifact {
        path: PathBuf::from("a"),
        project_name: "a".to_string(),
        ecosystem: Ecosystem::Node,
        sub_paths: vec![],
    });
    app.add_discovered(DiscoveredArtifact {
        path: PathBuf::from("b"),
        project_name: "b".to_string(),
        ecosystem: Ecosystem::Rust,
        sub_paths: vec![],
    });

    // 1. Lock when modal is active
    app.modal = ModalState::ConfirmDelete { count: 1, total_bytes: 100 };
    app.navigate_down();
    assert_eq!(app.cursor, 0, "Navigation must be locked while modal is active");
    app.toggle_select();
    assert!(!app.items[0].selected, "Selection must be locked while modal is active");
    app.select_all();
    assert!(!app.items[0].selected && !app.items[1].selected);
    app.cycle_sort();
    assert_eq!(app.sort_mode, SortMode::SizeDesc, "Sorting must be locked while modal is active");

    app.close_modal();

    // 2. Lock when is_busy is true
    app.is_busy = true;
    app.navigate_down();
    assert_eq!(app.cursor, 0, "Navigation must be locked while busy");
    app.toggle_select();
    assert!(!app.items[0].selected, "Selection must be locked while busy");
    app.open_delete_modal();
    assert_eq!(app.modal, ModalState::None, "Modal opening must be locked while busy");
    app.is_busy = false;
}

#[test]
fn test_keymap_events() {
    let mut app = App::new(PathBuf::from("."), false);
    app.add_discovered(DiscoveredArtifact {
        path: PathBuf::from("a"),
        project_name: "a".to_string(),
        ecosystem: Ecosystem::Node,
        sub_paths: vec![],
    });
    app.add_discovered(DiscoveredArtifact {
        path: PathBuf::from("b"),
        project_name: "b".to_string(),
        ecosystem: Ecosystem::Rust,
        sub_paths: vec![],
    });

    // Down key
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(app.cursor, 1);

    // Up key
    app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(app.cursor, 0);

    // Space key toggles selection
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert!(app.items[0].selected);

    // 'a' key toggles all
    app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    assert!(app.items[0].selected && app.items[1].selected);

    // 's' key cycles sort
    app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    assert_eq!(app.sort_mode, SortMode::DateOldestFirst);

    // 'd' key opens delete modal
    app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
    assert!(matches!(app.modal, ModalState::ConfirmDelete { .. }));

    // 'n' key closes modal
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    assert_eq!(app.modal, ModalState::None);

    // 'q' sets should_quit
    assert!(!app.should_quit);
    app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    assert!(app.should_quit);
}

#[test]
fn test_format_bytes() {
    assert_eq!(format_bytes(500), "500 B");
    assert_eq!(format_bytes(1024), "1.00 KB");
    assert_eq!(format_bytes(1024 * 1024 * 5), "5.00 MB");
    assert_eq!(format_bytes(1024 * 1024 * 1024 * 3), "3.00 GB");
    assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024 * 2), "2.00 TB");
}

#[test]
fn test_item_view_formatting() {
    let mut app = App::new(PathBuf::from("."), false);
    app.add_discovered(DiscoveredArtifact {
        path: PathBuf::from("proj/node_modules"),
        project_name: "proj".to_string(),
        ecosystem: Ecosystem::Node,
        sub_paths: vec![],
    });

    let item = &app.items[0];
    assert_eq!(item.formatted_size(), "calculating...");
    assert_eq!(item.formatted_date(), "-");

    let dt = chrono::Local::now();
    app.update_size(SizeResult {
        path: PathBuf::from("proj/node_modules"),
        size_bytes: 2048,
        last_modified: Some(dt),
    });

    let item = &app.items[0];
    assert_eq!(item.formatted_size(), "2.00 KB");
    assert_eq!(item.formatted_date(), dt.format("%Y-%m-%d %H:%M").to_string());
}

#[test]
fn test_app_metrics_helpers() {
    let mut app = App::new(PathBuf::from("."), false);
    assert_eq!(app.total_items(), 0);
    assert_eq!(app.total_bytes(), 0);
    assert_eq!(app.selected_count(), 0);
    assert_eq!(app.selected_bytes(), 0);

    app.add_discovered(DiscoveredArtifact {
        path: PathBuf::from("a"),
        project_name: "a".to_string(),
        ecosystem: Ecosystem::Node,
        sub_paths: vec![],
    });
    app.add_discovered(DiscoveredArtifact {
        path: PathBuf::from("b"),
        project_name: "b".to_string(),
        ecosystem: Ecosystem::Rust,
        sub_paths: vec![],
    });

    app.update_size(SizeResult {
        path: PathBuf::from("a"),
        size_bytes: 100,
        last_modified: None,
    });
    app.update_size(SizeResult {
        path: PathBuf::from("b"),
        size_bytes: 250,
        last_modified: None,
    });

    assert_eq!(app.total_items(), 2);
    assert_eq!(app.total_bytes(), 350);

    app.toggle_select(); // selects item a
    assert_eq!(app.selected_count(), 1);
    assert_eq!(app.selected_bytes(), 100);

    app.select_all(); // selects all
    assert_eq!(app.selected_count(), 2);
    assert_eq!(app.selected_bytes(), 350);
}

#[test]
fn test_extended_keymap_navigation() {
    let mut app = App::new(PathBuf::from("."), false);
    for i in 0..25 {
        app.add_discovered(DiscoveredArtifact {
            path: PathBuf::from(format!("item_{}", i)),
            project_name: format!("item_{}", i),
            ecosystem: Ecosystem::Node,
            sub_paths: vec![],
        });
    }

    assert_eq!(app.cursor, 0);

    // PageDown advances by 10
    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
    assert_eq!(app.cursor, 10);

    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
    assert_eq!(app.cursor, 20);

    // End key jumps to last item
    app.handle_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
    assert_eq!(app.cursor, 24);

    // PageUp moves up by 10
    app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
    assert_eq!(app.cursor, 14);

    // Home key jumps to first item
    app.handle_key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
    assert_eq!(app.cursor, 0);

    // Vim keys 'j' and 'k'
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(app.cursor, 1);
    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
    assert_eq!(app.cursor, 0);
}

#[test]
fn test_deletion_with_partial_failure() {
    let tmp = tempdir().unwrap();
    let folder = tmp.path().join("real_folder");
    create_dir_all(&folder).unwrap();
    write(folder.join("data.txt"), "xyz").unwrap();

    let mut app = App::new(PathBuf::from("."), false);
    // Add real folder
    app.add_discovered(DiscoveredArtifact {
        path: folder.clone(),
        project_name: "real".to_string(),
        ecosystem: Ecosystem::Node,
        sub_paths: vec![],
    });
    // Add missing/deleted folder
    app.add_discovered(DiscoveredArtifact {
        path: tmp.path().join("non_existent_folder"),
        project_name: "missing".to_string(),
        ecosystem: Ecosystem::Rust,
        sub_paths: vec![],
    });

    app.update_size(SizeResult {
        path: folder.clone(),
        size_bytes: 100,
        last_modified: None,
    });
    app.update_size(SizeResult {
        path: tmp.path().join("non_existent_folder"),
        size_bytes: 200,
        last_modified: None,
    });

    // Select all and execute
    app.select_all();
    app.open_delete_modal();
    app.confirm_delete_and_execute();

    // The real folder should be deleted
    assert!(!folder.exists());
    // total_freed_bytes should include the successfully deleted folder
    assert_eq!(app.total_freed_bytes, 100);
    // The failed item should remain in items list
    assert_eq!(app.items.len(), 1);
    assert_eq!(app.items[0].project_name, "missing");
    // Modal should inform user of error
    if let ModalState::Message { title, text } = &app.modal {
        assert!(title.contains("Failed") || title.contains("Error"));
        assert!(text.contains("errors") || text.contains("Path does not exist"));
    } else {
        panic!("Expected ModalState::Message with error details");
    }
}
