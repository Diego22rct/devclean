use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use devclean::app::App;
use devclean::scanner;
use devclean::sizer;
use devclean::ui;

/// Fast, multi-ecosystem project artifact cleaner TUI
#[derive(Parser, Debug)]
#[command(
    name = "devclean",
    version,
    about = "Fast, multi-ecosystem project artifact cleaner TUI"
)]
struct Cli {
    /// Root directory to scan (defaults to current directory)
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Run in simulation mode without deleting any files
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

/// RAII guard ensuring the terminal is cleanly restored on exit or unexpected error.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            std::io::stdout(),
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let root_path = dunce::canonicalize(&cli.path).unwrap_or(cli.path);
    if !root_path.exists() {
        eprintln!(
            "Error: Specified directory does not exist: {}",
            root_path.display()
        );
        std::process::exit(1);
    }

    // Set panic hook so terminal is restored before printing panic diagnostics
    let original_panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            std::io::stdout(),
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
        original_panic_hook(panic_info);
    }));

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, crossterm::cursor::Hide)?;
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Initialize application state
    let mut app = App::new(root_path.clone(), cli.dry_run);

    // Channels for streaming scan discovery and sizing calculation
    let (scan_tx, scan_rx) = crossbeam_channel::unbounded();
    let (sizer_in_tx, sizer_in_rx) = crossbeam_channel::unbounded();
    let (sizer_out_tx, sizer_out_rx) = crossbeam_channel::unbounded();

    // Spawn parallel sizer workers
    let _sizer_handle = sizer::spawn_artifact_sizer_workers(sizer_in_rx, sizer_out_tx);

    // Channels and background worker for asynchronous deletion
    let (deleter_task_tx, deleter_task_rx) =
        crossbeam_channel::unbounded::<Vec<devclean::app::DeletionTask>>();
    let (deleter_res_tx, deleter_res_rx) =
        crossbeam_channel::unbounded::<devclean::app::DeletionProgress>();

    let _deleter_handle = std::thread::Builder::new()
        .name("deleter".to_string())
        .spawn(move || {
            while let Ok(tasks) = deleter_task_rx.recv() {
                let total = tasks.len();
                for (idx, task) in tasks.into_iter().enumerate() {
                    let targets: Vec<&std::path::Path> =
                        task.target_paths.iter().map(|p| p.as_path()).collect();
                    let res = devclean::deleter::delete_paths(
                        &targets,
                        task.reported_bytes,
                        task.dry_run,
                    );
                    let freed_bytes = match &res {
                        devclean::deleter::DeletionResult::Success { freed_bytes } => *freed_bytes,
                        devclean::deleter::DeletionResult::Failed { .. } => 0,
                    };
                    let progress = devclean::app::DeletionProgress {
                        completed: idx + 1,
                        total,
                        freed_bytes,
                        result: res,
                        path: task.path,
                    };
                    let _ = deleter_res_tx.send(progress);
                }
            }
        })
        .expect("Failed to spawn deleter background thread");

    // Spawn background directory scanner thread
    let scan_root = root_path;
    let (scan_done_tx, scan_done_rx) = crossbeam_channel::bounded::<()>(1);
    let _scanner_handle = std::thread::Builder::new()
        .name("scanner".to_string())
        .spawn(move || {
            scanner::scan_directory(&scan_root, scan_tx);
            let _ = scan_done_tx.send(());
        })
        .expect("Failed to spawn scanner background thread");

    // Main interactive event loop
    loop {
        // Drain newly discovered items from scanner
        while let Ok(artifact) = scan_rx.try_recv() {
            let _ = sizer_in_tx.send(artifact.clone());
            app.add_discovered(artifact);
        }

        // Check if directory scan has completed
        if !app.scanning_done && scan_done_rx.try_recv().is_ok() {
            while let Ok(artifact) = scan_rx.try_recv() {
                let _ = sizer_in_tx.send(artifact.clone());
                app.add_discovered(artifact);
            }
            app.scanning_done = true;
        }

        // Drain calculated sizes from workers
        while let Ok(size_res) = sizer_out_rx.try_recv() {
            app.update_size(size_res);
        }

        // Dispatch pending deletion tasks to background worker
        if let Some(tasks) = app.take_pending_deletions() {
            let _ = deleter_task_tx.send(tasks);
        }

        // Drain deletion progress updates from background worker
        while let Ok(progress) = deleter_res_rx.try_recv() {
            app.apply_deletion_progress(progress);
        }

        // Render current TUI frame
        terminal.draw(|frame| {
            ui::render(frame, &app);
        })?;

        if app.should_quit {
            break;
        }

        // Poll keyboard input events
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.handle_key(key);
                }
            }
        }
    }

    Ok(())
}
