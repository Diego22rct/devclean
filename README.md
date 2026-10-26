# 🧹 devclean

> Fast, interactive, multi-ecosystem developer artifact and cache cleaner written in Rust.

`devclean` traverses your workspaces at native I/O speeds on Windows, Linux, and macOS, detects regenerable build outputs, dependency directories, and cache folders across **Node.js**, **Flutter / Dart**, **Rust / Cargo**, **.NET / C#**, and **Python**, calculates folder sizes in the background without UI stutter, and lets you safely purge them via an interactive Terminal UI (TUI).

---

## ✨ Features

- **🚀 Native Multithreaded Traversal**: Built on `jwalk` and `rayon` to scan deep directory hierarchies with 100,000+ files in seconds on Windows NTFS.
- **⚡ Non-Blocking UI**: Asynchronous size and timestamp calculation streams real-time updates directly into the UI without freezing.
- **🛡️ Contextual Safety Guards**:
  - Never touches or descends into `.git`.
  - Contextual validation prevents false positives (e.g. `.NET` `bin`/`obj` folders are only flagged if they contain compiler output signatures like `Debug`/`Release`/`net*`, protecting Node CLI tools with `bin/cli.js`).
  - Monorepo and nested project boundary awareness (e.g., `src-tauri/target` is attributed to Rust even when inside a Node monorepo).
  - Python `__pycache__` directories within a project are aggregated into a single row.
- **🧪 Safe Dry-Run Mode (`--dry-run`)**: Test and navigate in interactive simulation mode without modifying any files on disk.
- **🎛️ Interactive Terminal UI**: Full keyboard navigation, sorting by size/date/path/ecosystem, batch selection, confirmation dialogs, and background deletion with a progress bar.

---

## 📦 Supported Ecosystems

| Ecosystem | Project Manifest | Detected Artifacts |
|---|---|---|
| **Node.js / TS / JS** | `package.json` | `node_modules`, `.next`, `.turbo`, `dist`, `build`, `.nuxt` |
| **Flutter / Dart** | `pubspec.yaml` | `.dart_tool`, `build`, `android/.gradle`, `android/app/build`, `windows/flutter/ephemeral` |
| **Rust / Cargo** | `Cargo.toml` | `target` |
| **.NET / C#** | `*.sln`, `*.csproj`, `*.fsproj` | `bin`, `obj` *(validated compiler outputs)* |
| **Python** | `requirements.txt`, `pyproject.toml`, etc. | `__pycache__` *(aggregated)*, `.venv`, `venv`, `.pytest_cache`, `.ruff_cache` |

---

## 🚀 Installation

Ensure you have Rust and Cargo installed:

```bash
git clone https://github.com/Diego22rct/devclean.git
cd devclean
cargo install --path .
```

`devclean.exe` will be available directly in your terminal.

---

## 💻 Usage

### Interactive Clean
Scan the current directory:
```bash
devclean
```

Scan a specific workspace or folder:
```bash
devclean C:\Dev
```

### Simulation / Dry-Run Mode
Simulate scanning and deletion without touching disk:
```bash
devclean --dry-run
```

---

## ⌨️ Keybindings

| Key | Action |
|---|---|
| `↑` / `k` | Move cursor up |
| `↓` / `j` | Move cursor down |
| `Space` | Toggle selection for the focused row |
| `a` | Select / deselect all visible artifacts |
| `s` | Cycle sort order (Size ↓, Date oldest ↑, Path, Ecosystem) |
| `d` | Open deletion confirmation modal |
| `y` / `n` | Confirm or cancel deletion in modal |
| `q` / `Esc` | Quit and restore terminal |

---

## 🧪 Testing

Run the full automated test suite:

```bash
cargo test
```

67 automated unit and integration tests covering multi-threaded scanning, project boundary scoping, asynchronous sizing, resilient file deletion, and state transitions.

---

## 📄 License

MIT
