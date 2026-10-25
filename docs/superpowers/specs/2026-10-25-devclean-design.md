# Design Document: `devclean`

**Date:** 2026-10-25  
**Topic:** Multi-Ecosystem Project Artifact Cleaner TUI in Rust  
**Status:** Approved for Planning  

---

## 1. Intent & The Why

Developers working across multiple tech stacks accumulate massive amounts of auto-generated, regenerable build outputs, dependency directories, and cache files over time. In a single development directory (`C:\Dev`), projects from hackathons, personal experiments, client freelance work, and practice exercises quickly consume tens of gigabytes of disk space.

Existing tools like `npkill` only clean Node.js `node_modules` and suffer from slow traversal on Windows NTFS. Other single-language tools (like `cargo-sweep` or `flutter clean`) require navigating into each project individually and running separate commands.

`devclean` is a fast, cross-ecosystem, standalone terminal UI (TUI) tool written in Rust for Windows (and cross-platform). It scans directory trees concurrently, accurately identifies project boundaries across Node/TypeScript, Flutter/Dart, Rust/Cargo, .NET/C#, and Python, calculates folder sizes in the background without UI stutter, and provides an interactive interface to selectively or batch-purge reclaimable artifacts.

---

## 2. Goals, Non-Goals & Anti-Goals

### Goals
- **Multi-Ecosystem Discovery**: Automatically discover regenerable build/cache/dependency directories for:
  - **Node.js / TypeScript / JavaScript**: `node_modules`, `.next`, `.turbo`, `dist`, `build`, `.nuxt`
  - **Flutter / Dart**: `.dart_tool`, `build`, `android/.gradle`, `android/app/build`, `windows/flutter/ephemeral`
  - **Rust / Cargo**: `target`
  - **.NET / C#**: `bin`, `obj`
  - **Python**: `__pycache__`, `.venv`, `venv`, `.pytest_cache`, `.ruff_cache`, `.mypy_cache`
- **Monorepo & Nested Project Awareness**: Correctly handle monorepos (e.g., Turborepo, pnpm workspaces, Flutter apps with Android subdirectories, Tauri apps with `src-tauri/target`) without misidentifying or duplicating project scopes.
- **Fast Multithreaded I/O**: Utilize concurrent file-system traversal (`jwalk` or `rayon`) so that traversing trees with 100,000+ files on Windows NTFS takes seconds rather than minutes.
- **Unified Interactive TUI**: All interactions happen directly in the terminal UI (including a simulated dry-run mode). The UI is immediately interactive; item sizes are calculated asynchronously on background threads and stream into the view as they complete.
- **Standalone Binary**: Compiles to a single, portable executable (`devclean.exe`) with zero runtime dependencies (no Node, Python, or runtime installs required).
- **Safety by Default**: Confirmations required before destructive actions; explicit total space reclaimed reported; strict contextual validation before deletion.

### Non-Goals
- Cleaning Git history or purging uncommitted Git branches.
- Cleaning global system-level caches (e.g., global npm cache in `AppData`, global Cargo cache in `~/.cargo`, system temp files).
- Modifying project source code or configuration files.
- Non-interactive headless CLI script modes (the tool is purpose-built as an interactive TUI).

### Anti-Goals (What would make this a failure)
- **Accidental Deletion of Source Code**: Never delete any directory that is not strictly an artifact folder. Never delete `.git`.
- **False Positive Root Purges**: A generic folder named `build` or `bin` that is NOT inside a detected project marker must never be flagged for deletion.
- **UI Freezes**: Never block the main terminal rendering loop while computing recursive folder sizes or deleting files.

---

## 3. Approaches Considered & Trade-offs

1. **Rust Native Binary (`ratatui` + `jwalk`) — Selected**
   - *Pros*: Highest possible traversal and deletion speed on Windows NTFS; single standalone `.exe` easily placed in `PATH`; minimal memory footprint; fully non-blocking asynchronous architecture.
   - *Cons*: Initial compile time required to build dependencies.

2. **Node.js / TypeScript CLI (`ink` / `prompts`) — Rejected**
   - *Pros*: Fast initial development; familiar to web developers.
   - *Cons*: Single-threaded JS event loop and high Windows NTFS overhead make scanning deep trees with hundreds of thousands of files sluggish; requires Node.js runtime.

3. **Python Script (`textual` / `rich`) — Rejected**
   - *Pros*: Simple to script.
   - *Cons*: Slowest traversal speed on Windows; requires Python environment and external pip dependencies.

---

## 4. Architectural & System Design

### 4.1 Scanner & Discovery Engine
- **Directory Traversal**: Configurable scan root (defaults to current working directory `.`, e.g. `C:\Dev`).
- **Project Boundary Detection**:
  - Traverses down directory trees, skipping descent into already identified artifact directories (e.g., do not recurse into `node_modules` or `target` looking for other projects).
  - Recognizes project signatures:
    - `package.json` -> Node.js ecosystem
    - `pubspec.yaml` -> Flutter/Dart ecosystem
    - `Cargo.toml` -> Rust ecosystem
    - `*.sln`, `*.csproj`, `*.fsproj` -> .NET ecosystem
    - `pyproject.toml`, `requirements.txt`, `Pipfile`, `setup.py` -> Python ecosystem

### 4.2 Contextual Artifact Safety Rules (Builder Call)
To eliminate any risk of deleting user source code in folders with generic names:
- **.NET `bin` and `obj`**: Only flagged if they are direct children of a directory containing a `.csproj` or `.fsproj`, AND contain standard compiler subfolders (e.g. `Debug`, `Release`, `net*`). Node CLI packages with `bin/cli.js` are never matched.
- **Node `dist` and `build`**: Only flagged if direct children of a directory with `package.json` and lacking any inner `package.json`.
- **Flutter `build`**: Only flagged if direct child of a Flutter project root (with `pubspec.yaml`) or `android/app/build`.
- **Rust `target`**: Only flagged if direct child of a directory containing `Cargo.toml`.
- **Git Safety**: Never flag or descend into `.git`.

### 4.3 Monorepo Scoping, Deduplication & Aggregation (Builder Call)
- **Path Deduplication**: An artifact is tracked by its canonical normalized absolute path in a `HashSet<PathBuf>`. No artifact path can appear more than once or double-count disk space.
- **Closest Manifest Attribution**: In nested projects (e.g. `constancia/contract/target` containing a `Cargo.toml` within a Next.js monorepo), the artifact is attributed to the closest project manifest (`[Rust]`).
- **Ubiquitous Artifact Aggregation**: Python `__pycache__` folders within a given project are aggregated into a single composite row in the TUI (`[Python] <project>: __pycache__ (all)`), preventing dozens of tiny rows from cluttering the table.

### 4.4 Asynchronous Size Calculation Pipeline
- Scanned artifact candidates are emitted through an unbounded crossbeam channel to the TUI state immediately upon discovery.
- A background worker pool (`rayon`) receives candidate paths and computes recursive file sizes and last modified timestamps.
- Updates are streamed back to the TUI state. Items display `calculating...` until their size is resolved.

### 4.5 Interactive Terminal User Interface (TUI)
- Framework: `ratatui` + `crossterm`.
- **Header**:
  - Current scan root, total projects discovered, total reclaimable space found so far, scanning status indicator.
  - Mode badge: `[LIVE]` or `[SIMULATION / DRY-RUN]`.
- **Table / List View**:
  - Columns:
    - Selection `[ ]` / `[x]`
    - Ecosystem Tag (`[Node]`, `[Flutter]`, `[Rust]`, `[.NET]`, `[Python]`) with distinct colors
    - Project & Relative Path
    - Size (Human-readable: B, KB, MB, GB)
    - Last Modified Date (Relative, e.g. `3 months ago`, or YYYY-MM-DD)
- **Footer / Keymap**:
  - `↑`/`k`, `↓`/`j`: Navigate rows
  - `Space`: Toggle row selection
  - `a`: Select / deselect all visible
  - `d`: Delete selected items (opens confirmation modal)
  - `s`: Cycle sort order (by Size descending, by Date oldest first, by Path, by Ecosystem)
  - `r`: Rescan
  - `q` / `Esc`: Exit
- **Confirmation Modal**:
  - Appears when `d` is pressed.
  - Summarizes: Number of folders selected and total space to be freed.
  - Asks: `Are you sure you want to delete these X folders? [y/N]`
  - In normal mode: deletions run on background worker threads with progress indicator (`Deleting 3/12...`).
  - In `--dry-run` mode: simulates the deletion, clears the rows from the view, and shows: `[Simulation] Would have freed X GB (no files deleted)`.

---

## 5. What is Left to the Builder

- Exact crate versions (`ratatui = "0.29"`, `crossterm = "0.28"`, `jwalk = "0.8"`, `rayon = "1.10"`, `crossbeam-channel = "0.5"`, `clap = { version = "4", features = ["derive"] }`, `chrono = "0.4"`).
- Exact UI styling (borders, color scheme per ecosystem badge).
- Handling Windows extended-length paths (`\\?\`) for paths exceeding 260 characters.
