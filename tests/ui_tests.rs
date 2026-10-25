use devclean::app::{App, ModalState};
use devclean::ecosystem::Ecosystem;
use devclean::scanner::DiscoveredArtifact;
use devclean::sizer::SizeResult;
use devclean::ui::{format_bytes, render};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::path::PathBuf;

#[test]
fn test_format_bytes() {
    assert_eq!(format_bytes(500), "500 B");
    assert_eq!(format_bytes(1024), "1.00 KB");
    assert_eq!(format_bytes(1024 * 1024 * 5), "5.00 MB");
    assert_eq!(format_bytes(1024 * 1024 * 1024 * 3), "3.00 GB");
    assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024 * 2), "2.00 TB");
}

#[test]
fn test_render_empty_app() {
    let app = App::new(PathBuf::from("C:\\Dev\\myproject"), true);
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            render(frame, &app);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let content = format!("{:?}", buffer);

    // Verify header contains title, dry-run badge, and root
    assert!(content.contains("devclean"));
    assert!(content.contains("SIMULATION / DRY-RUN") || content.contains("DRY-RUN"));
}

#[test]
fn test_render_with_items_and_live_mode() {
    let mut app = App::new(PathBuf::from("/test/root"), false);
    app.add_discovered(DiscoveredArtifact {
        path: PathBuf::from("/test/root/web/node_modules"),
        project_name: "web".to_string(),
        ecosystem: Ecosystem::Node,
        sub_paths: vec![],
    });
    app.add_discovered(DiscoveredArtifact {
        path: PathBuf::from("/test/root/core/target"),
        project_name: "core".to_string(),
        ecosystem: Ecosystem::Rust,
        sub_paths: vec![],
    });
    app.update_size(SizeResult {
        path: PathBuf::from("/test/root/web/node_modules"),
        size_bytes: 52_428_800, // 50 MB
        last_modified: None,
    });

    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            render(frame, &app);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let content = format!("{:?}", buffer);

    assert!(content.contains("LIVE"));
    assert!(content.contains("web"));
    assert!(content.contains("core"));
    assert!(content.contains("50.00 MB"));
}

#[test]
fn test_render_delete_modal() {
    let mut app = App::new(PathBuf::from("/test/root"), false);
    app.modal = ModalState::ConfirmDelete {
        count: 2,
        total_bytes: 104_857_600, // 100 MB
    };

    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            render(frame, &app);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let content = format!("{:?}", buffer);

    assert!(content.contains("Confirm"));
    assert!(content.contains("100.00 MB"));
}

#[test]
fn test_render_message_modal() {
    let mut app = App::new(PathBuf::from("/test/root"), false);
    app.modal = ModalState::Message {
        title: "Test Popup".to_string(),
        text: "Important operation completed successfully.".to_string(),
    };

    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            render(frame, &app);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let content = format!("{:?}", buffer);

    assert!(content.contains("Test Popup"));
    assert!(content.contains("Important operation completed"));
}

#[test]
fn test_render_empty_app_artifact_path_column() {
    let mut app = App::new(PathBuf::from("C:\\Dev\\project"), false);
    app.scanning_done = true;

    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            render(frame, &app);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let content = format!("{:?}", buffer);

    assert!(content.contains("No regenerable build artifacts discovered."));

    // Now test scanning state
    app.scanning_done = false;
    terminal
        .draw(|frame| {
            render(frame, &app);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let content = format!("{:?}", buffer);
    assert!(content.contains("Scanning directories for build artifacts..."));
}

#[test]
fn test_render_deleting_modal() {
    let mut app = App::new(PathBuf::from("/test/root"), false);
    app.modal = ModalState::Deleting {
        completed: 2,
        total: 5,
        current_item: "web/node_modules".to_string(),
    };

    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            render(frame, &app);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let content = format!("{:?}", buffer);

    assert!(content.contains("Deleting [2/5]"));
    assert!(content.contains("web/node_modules"));
    assert!(content.contains("40%"));
    assert!(content.contains("Please wait while artifacts are deleted..."));
}

#[test]
fn test_render_multiline_message_modal() {
    let mut app = App::new(PathBuf::from("/test/root"), false);
    app.modal = ModalState::Message {
        title: "Deletion Complete (Errors Encountered)".to_string(),
        text: "Encountered errors:\nFailed to delete dir 1: Access denied\nFailed to delete dir 2: File locked".to_string(),
    };

    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            render(frame, &app);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let content = format!("{:?}", buffer);

    assert!(content.contains("Deletion Complete (Errors Encountered)"));
    assert!(content.contains("Encountered errors:"));
    assert!(content.contains("Failed to delete dir 1: Access denied"));
    assert!(content.contains("Failed to delete dir 2: File locked"));
}
