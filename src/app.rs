use std::path::{Path, PathBuf};
use crossterm::event::{KeyCode, KeyEvent};

use crate::deleter;
use crate::ecosystem::Ecosystem;
use crate::scanner::DiscoveredArtifact;
use crate::sizer::SizeResult;

/// Formats a byte quantity into a human-readable string (B, KB, MB, GB, TB).
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Sort modes supported for organizing discovered artifacts in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortMode {
    #[default]
    SizeDesc,
    DateOldestFirst,
    PathAsc,
    EcosystemAsc,
}

impl SortMode {
    /// Advances to the next sort mode in the cycle.
    pub fn next(&self) -> Self {
        match self {
            SortMode::SizeDesc => SortMode::DateOldestFirst,
            SortMode::DateOldestFirst => SortMode::PathAsc,
            SortMode::PathAsc => SortMode::EcosystemAsc,
            SortMode::EcosystemAsc => SortMode::SizeDesc,
        }
    }

    /// User-friendly label for current sort mode.
    pub fn label(&self) -> &'static str {
        match self {
            SortMode::SizeDesc => "Size (Largest first)",
            SortMode::DateOldestFirst => "Date (Oldest first)",
            SortMode::PathAsc => "Path (A-Z)",
            SortMode::EcosystemAsc => "Ecosystem (A-Z)",
        }
    }
}

/// Represents the active modal overlay state in the TUI.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ModalState {
    #[default]
    None,
    ConfirmDelete {
        count: usize,
        total_bytes: u64,
    },
    Deleting {
        completed: usize,
        total: usize,
        current_item: String,
    },
    Message {
        title: String,
        text: String,
    },
}

/// A background deletion task representation for a single discovered artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletionTask {
    pub path: PathBuf,
    pub target_paths: Vec<PathBuf>,
    pub reported_bytes: u64,
    pub display_name: String,
    pub dry_run: bool,
}

/// Progress update message streamed from the background deleter thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletionProgress {
    pub completed: usize,
    pub total: usize,
    pub freed_bytes: u64,
    pub result: deleter::DeletionResult,
    pub path: PathBuf,
}

/// Tracking state for an in-flight background deletion operation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActiveDeletion {
    pub tasks: Vec<DeletionTask>,
    pub freed_bytes: u64,
    pub errors: Vec<String>,
    pub successful_paths: Vec<PathBuf>,
}

/// Presentation model for a single artifact item in the TUI list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemView {
    pub path: PathBuf,
    pub project_name: String,
    pub ecosystem: Ecosystem,
    pub sub_paths: Vec<PathBuf>,
    pub size_bytes: Option<u64>,
    pub last_modified: Option<chrono::DateTime<chrono::Local>>,
    pub selected: bool,
}

impl ItemView {
    /// Creates a new ItemView with uninitialized size and selection.
    pub fn new(
        path: PathBuf,
        project_name: String,
        ecosystem: Ecosystem,
        sub_paths: Vec<PathBuf>,
    ) -> Self {
        Self {
            path,
            project_name,
            ecosystem,
            sub_paths,
            size_bytes: None,
            last_modified: None,
            selected: false,
        }
    }

    /// Returns the physical target paths associated with this artifact item.
    pub fn target_paths(&self) -> Vec<&Path> {
        if self.sub_paths.is_empty() {
            vec![&self.path]
        } else {
            self.sub_paths.iter().map(|p| p.as_path()).collect()
        }
    }

    /// Formatted size string for display.
    pub fn formatted_size(&self) -> String {
        match self.size_bytes {
            Some(bytes) => format_bytes(bytes),
            None => "calculating...".to_string(),
        }
    }

    /// Formatted date string for display.
    pub fn formatted_date(&self) -> String {
        match self.last_modified {
            Some(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
            None => "-".to_string(),
        }
    }
}

impl From<DiscoveredArtifact> for ItemView {
    fn from(artifact: DiscoveredArtifact) -> Self {
        Self::new(
            artifact.path,
            artifact.project_name,
            artifact.ecosystem,
            artifact.sub_paths,
        )
    }
}

/// Central application state machine.
pub struct App {
    pub root_path: PathBuf,
    pub items: Vec<ItemView>,
    pub cursor: usize,
    pub sort_mode: SortMode,
    pub modal: ModalState,
    pub dry_run: bool,
    pub scanning_done: bool,
    pub total_freed_bytes: u64,
    pub is_busy: bool,
    pub status_message: Option<String>,
    pub should_quit: bool,
    pub pending_deletion_tasks: Option<Vec<DeletionTask>>,
    pub active_deletion: Option<ActiveDeletion>,
}

impl App {
    /// Creates a new App state initialized with the given root scan path and dry-run flag.
    pub fn new(root_path: PathBuf, dry_run: bool) -> Self {
        Self {
            root_path,
            items: Vec::new(),
            cursor: 0,
            sort_mode: SortMode::SizeDesc,
            modal: ModalState::None,
            dry_run,
            scanning_done: false,
            total_freed_bytes: 0,
            is_busy: false,
            status_message: None,
            should_quit: false,
            pending_deletion_tasks: None,
            active_deletion: None,
        }
    }

    /// Clamps the cursor within valid item bounds.
    pub fn adjust_cursor(&mut self) {
        if self.items.is_empty() {
            self.cursor = 0;
        } else if self.cursor >= self.items.len() {
            self.cursor = self.items.len() - 1;
        }
    }

    /// Moves cursor up by one item, respecting boundaries and active modal locking.
    pub fn navigate_up(&mut self) {
        if self.modal != ModalState::None || self.is_busy {
            return;
        }
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Moves cursor down by one item, respecting boundaries and active modal locking.
    pub fn navigate_down(&mut self) {
        if self.modal != ModalState::None || self.is_busy {
            return;
        }
        if !self.items.is_empty() && self.cursor + 1 < self.items.len() {
            self.cursor += 1;
        }
    }

    /// Toggles selection of the item currently under the cursor.
    pub fn toggle_select(&mut self) {
        if self.modal != ModalState::None || self.is_busy {
            return;
        }
        if let Some(item) = self.items.get_mut(self.cursor) {
            item.selected = !item.selected;
        }
    }

    /// Toggles selection of all items: if all are selected, deselects all; otherwise selects all.
    pub fn select_all(&mut self) {
        if self.modal != ModalState::None || self.is_busy {
            return;
        }
        let all_selected = !self.items.is_empty() && self.items.iter().all(|item| item.selected);
        let target = !all_selected;
        for item in &mut self.items {
            item.selected = target;
        }
    }

    /// Advances to the next sort mode and re-sorts items.
    pub fn cycle_sort(&mut self) {
        if self.modal != ModalState::None || self.is_busy {
            return;
        }
        self.sort_mode = self.sort_mode.next();
        self.sort_items();
    }

    /// Sorts items according to the current sort mode, preserving cursor selection position.
    pub fn sort_items(&mut self) {
        if self.items.is_empty() {
            self.cursor = 0;
            return;
        }

        let current_path = self.items.get(self.cursor).map(|i| i.path.clone());

        match self.sort_mode {
            SortMode::SizeDesc => {
                self.items.sort_by(|a, b| {
                    match (a.size_bytes, b.size_bytes) {
                        (Some(as_), Some(bs)) => bs.cmp(&as_).then_with(|| a.path.cmp(&b.path)),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => a.path.cmp(&b.path),
                    }
                });
            }
            SortMode::DateOldestFirst => {
                self.items.sort_by(|a, b| {
                    match (a.last_modified, b.last_modified) {
                        (Some(am), Some(bm)) => am.cmp(&bm).then_with(|| a.path.cmp(&b.path)),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => a.path.cmp(&b.path),
                    }
                });
            }
            SortMode::PathAsc => {
                self.items.sort_by(|a, b| a.path.cmp(&b.path));
            }
            SortMode::EcosystemAsc => {
                self.items.sort_by(|a, b| {
                    a.ecosystem
                        .name()
                        .cmp(b.ecosystem.name())
                        .then_with(|| a.path.cmp(&b.path))
                });
            }
        }

        if let Some(path) = current_path {
            if let Some(new_idx) = self.items.iter().position(|i| i.path == path) {
                self.cursor = new_idx;
            } else {
                self.adjust_cursor();
            }
        } else {
            self.adjust_cursor();
        }
    }

    /// Adds a newly discovered artifact to the items collection if not already present.
    pub fn add_discovered(&mut self, artifact: DiscoveredArtifact) {
        if self.items.iter().any(|item| item.path == artifact.path) {
            return;
        }
        self.items.push(ItemView::from(artifact));
        self.adjust_cursor();
    }

    /// Updates the size and last modified timestamp of an artifact item.
    pub fn update_size(&mut self, result: SizeResult) {
        if let Some(item) = self.items.iter_mut().find(|item| item.path == result.path) {
            item.size_bytes = Some(result.size_bytes);
            item.last_modified = result.last_modified;
        }
    }

    /// Opens the delete confirmation modal for selected items, or displays an info message if none are selected.
    pub fn open_delete_modal(&mut self) {
        if self.modal != ModalState::None || self.is_busy {
            return;
        }
        let count = self.selected_count();
        if count == 0 {
            self.modal = ModalState::Message {
                title: "Nothing Selected".to_string(),
                text: "No items selected for deletion. Press [Space] to select items, or [a] to select all.".to_string(),
            };
        } else {
            let total_bytes = self.selected_bytes();
            self.modal = ModalState::ConfirmDelete { count, total_bytes };
        }
    }

    /// Closes any open modal dialog.
    pub fn close_modal(&mut self) {
        self.modal = ModalState::None;
    }

    /// Initiates background deletion for all currently selected artifacts.
    /// Sets `is_busy` to true, transitions `modal` to `ModalState::Deleting`,
    /// and stages pending tasks to be dispatched by the event loop.
    pub fn start_deletion(&mut self) -> Option<Vec<DeletionTask>> {
        if self.is_busy {
            return None;
        }

        let selected_indices: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| if item.selected { Some(idx) } else { None })
            .collect();

        if selected_indices.is_empty() {
            self.modal = ModalState::None;
            return None;
        }

        let tasks: Vec<DeletionTask> = selected_indices
            .iter()
            .map(|&idx| {
                let item = &self.items[idx];
                let display_name = item
                    .path
                    .strip_prefix(&self.root_path)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| item.path.display().to_string());

                DeletionTask {
                    path: item.path.clone(),
                    target_paths: item.target_paths().into_iter().map(|p| p.to_path_buf()).collect(),
                    reported_bytes: item.size_bytes.unwrap_or(0),
                    display_name,
                    dry_run: self.dry_run,
                }
            })
            .collect();

        let total = tasks.len();
        let first_item = tasks[0].display_name.clone();

        self.is_busy = true;
        self.modal = ModalState::Deleting {
            completed: 0,
            total,
            current_item: first_item,
        };

        self.active_deletion = Some(ActiveDeletion {
            tasks: tasks.clone(),
            freed_bytes: 0,
            errors: Vec::new(),
            successful_paths: Vec::new(),
        });

        self.pending_deletion_tasks = Some(tasks.clone());

        Some(tasks)
    }

    /// Takes staged pending deletion tasks so they can be dispatched to the background worker.
    pub fn take_pending_deletions(&mut self) -> Option<Vec<DeletionTask>> {
        self.pending_deletion_tasks.take()
    }

    /// Receives a progress update from the background deletion worker and updates state.
    pub fn apply_deletion_progress(&mut self, progress: DeletionProgress) {
        let (is_done, next_item) = if let Some(active) = &mut self.active_deletion {
            match &progress.result {
                deleter::DeletionResult::Success { freed_bytes } => {
                    active.freed_bytes = active.freed_bytes.saturating_add(*freed_bytes);
                    active.successful_paths.push(progress.path.clone());
                }
                deleter::DeletionResult::Failed { error } => {
                    active.errors.push(error.clone());
                }
            }
            let is_done = progress.completed >= progress.total;
            let next_item = if !is_done && progress.completed < active.tasks.len() {
                Some(active.tasks[progress.completed].display_name.clone())
            } else {
                None
            };
            (is_done, next_item)
        } else {
            (progress.completed >= progress.total, None)
        };

        if is_done {
            self.finish_deletion();
        } else if let Some(item_name) = next_item {
            self.modal = ModalState::Deleting {
                completed: progress.completed,
                total: progress.total,
                current_item: item_name,
            };
        }
    }

    /// Finalizes the deletion operation, updates metrics, removes deleted items, and shows the summary modal.
    pub fn finish_deletion(&mut self) {
        let active = match self.active_deletion.take() {
            Some(a) => a,
            None => {
                self.is_busy = false;
                self.modal = ModalState::None;
                return;
            }
        };

        self.total_freed_bytes = self.total_freed_bytes.saturating_add(active.freed_bytes);

        // Remove successfully deleted items from self.items
        for path in &active.successful_paths {
            if let Some(idx) = self.items.iter().position(|i| i.path == *path) {
                self.items.remove(idx);
            }
        }
        self.adjust_cursor();

        let freed_str = format_bytes(active.freed_bytes);

        if self.dry_run {
            let msg = format!("[Simulation] Would have freed {}", freed_str);
            self.status_message = Some(msg.clone());
            self.modal = ModalState::Message {
                title: "Dry Run Simulation".to_string(),
                text: msg,
            };
        } else if active.errors.is_empty() {
            let msg = format!("Successfully freed {}", freed_str);
            self.status_message = Some(msg.clone());
            self.modal = ModalState::Message {
                title: "Deletion Complete".to_string(),
                text: msg,
            };
        } else {
            let err_summary = active.errors.join("\n");
            let msg = format!("Freed {}. Encountered {} errors.", freed_str, active.errors.len());
            self.status_message = Some(msg);
            self.modal = ModalState::Message {
                title: "Deletion Complete (Errors Encountered)".to_string(),
                text: format!("Encountered errors:\n{}", err_summary),
            };
        }

        self.is_busy = false;
    }

    /// Confirms and executes deletion (or simulation in dry-run mode) on all selected artifacts.
    pub fn confirm_delete_and_execute(&mut self) {
        if self.is_busy {
            return;
        }

        if let Some(tasks) = self.start_deletion() {
            let _ = self.take_pending_deletions();
            let total = tasks.len();
            for (idx, task) in tasks.into_iter().enumerate() {
                let targets: Vec<&Path> = task.target_paths.iter().map(|p| p.as_path()).collect();
                let result = deleter::delete_paths(&targets, task.reported_bytes, task.dry_run);
                let freed_bytes = match &result {
                    deleter::DeletionResult::Success { freed_bytes } => *freed_bytes,
                    deleter::DeletionResult::Failed { .. } => 0,
                };
                self.apply_deletion_progress(DeletionProgress {
                    completed: idx + 1,
                    total,
                    freed_bytes,
                    result,
                    path: task.path,
                });
            }
        }
    }

    /// Handles keyboard input events, routing actions according to modal state.
    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.is_busy {
            return;
        }

        match &self.modal {
            ModalState::ConfirmDelete { .. } => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    self.start_deletion();
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.close_modal();
                }
                _ => {}
            },
            ModalState::Deleting { .. } => {}
            ModalState::Message { .. } => match key.code {
                KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' ') => {
                    self.close_modal();
                }
                _ => {}
            },
            ModalState::None => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.navigate_up();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.navigate_down();
                }
                KeyCode::PageUp => {
                    self.cursor = self.cursor.saturating_sub(10);
                }
                KeyCode::PageDown => {
                    if !self.items.is_empty() {
                        self.cursor = (self.cursor + 10).min(self.items.len() - 1);
                    }
                }
                KeyCode::Home => {
                    self.cursor = 0;
                }
                KeyCode::End => {
                    if !self.items.is_empty() {
                        self.cursor = self.items.len() - 1;
                    }
                }
                KeyCode::Char(' ') => {
                    self.toggle_select();
                }
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    self.select_all();
                }
                KeyCode::Char('s') | KeyCode::Char('S') => {
                    self.cycle_sort();
                }
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    self.open_delete_modal();
                }
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    self.should_quit = true;
                }
                _ => {}
            },
        }
    }

    /// Returns the number of currently selected items.
    pub fn selected_count(&self) -> usize {
        self.items.iter().filter(|i| i.selected).count()
    }

    /// Returns the total byte size of currently selected items with calculated sizes.
    pub fn selected_bytes(&self) -> u64 {
        self.items
            .iter()
            .filter(|i| i.selected)
            .filter_map(|i| i.size_bytes)
            .sum()
    }

    /// Returns the total byte size of all items with calculated sizes.
    pub fn total_bytes(&self) -> u64 {
        self.items.iter().filter_map(|i| i.size_bytes).sum()
    }

    /// Returns the total number of discovered items.
    pub fn total_items(&self) -> usize {
        self.items.len()
    }
}
