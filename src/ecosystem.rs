use std::path::Path;

/// Supported development ecosystems
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ecosystem {
    Node,
    Flutter,
    Rust,
    DotNet,
    Python,
}

impl Ecosystem {
    /// Friendly display name for the ecosystem
    pub fn name(&self) -> &'static str {
        match self {
            Ecosystem::Node => "Node",
            Ecosystem::Flutter => "Flutter",
            Ecosystem::Rust => "Rust",
            Ecosystem::DotNet => ".NET",
            Ecosystem::Python => "Python",
        }
    }

    /// TUI badge label formatted with brackets
    pub fn tag(&self) -> &'static str {
        match self {
            Ecosystem::Node => "[Node]",
            Ecosystem::Flutter => "[Flutter]",
            Ecosystem::Rust => "[Rust]",
            Ecosystem::DotNet => "[.NET]",
            Ecosystem::Python => "[Python]",
        }
    }
}

impl std::fmt::Display for Ecosystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Rule identifying an artifact folder associated with an ecosystem
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRule {
    pub ecosystem: Ecosystem,
    pub target_name: &'static str,
    pub is_exact: bool,
}

/// Canonical artifact rules across supported ecosystems
pub const ARTIFACT_RULES: &[ArtifactRule] = &[
    // Node.js
    ArtifactRule { ecosystem: Ecosystem::Node, target_name: "node_modules", is_exact: true },
    ArtifactRule { ecosystem: Ecosystem::Node, target_name: ".next", is_exact: true },
    ArtifactRule { ecosystem: Ecosystem::Node, target_name: ".turbo", is_exact: true },
    ArtifactRule { ecosystem: Ecosystem::Node, target_name: "dist", is_exact: true },
    ArtifactRule { ecosystem: Ecosystem::Node, target_name: "build", is_exact: true },
    ArtifactRule { ecosystem: Ecosystem::Node, target_name: ".nuxt", is_exact: true },
    // Flutter
    ArtifactRule { ecosystem: Ecosystem::Flutter, target_name: ".dart_tool", is_exact: true },
    ArtifactRule { ecosystem: Ecosystem::Flutter, target_name: "build", is_exact: true },
    ArtifactRule { ecosystem: Ecosystem::Flutter, target_name: "ephemeral", is_exact: true },
    // Rust
    ArtifactRule { ecosystem: Ecosystem::Rust, target_name: "target", is_exact: true },
    // .NET
    ArtifactRule { ecosystem: Ecosystem::DotNet, target_name: "bin", is_exact: true },
    ArtifactRule { ecosystem: Ecosystem::DotNet, target_name: "obj", is_exact: true },
    // Python
    ArtifactRule { ecosystem: Ecosystem::Python, target_name: "__pycache__", is_exact: true },
    ArtifactRule { ecosystem: Ecosystem::Python, target_name: ".venv", is_exact: true },
    ArtifactRule { ecosystem: Ecosystem::Python, target_name: "venv", is_exact: true },
    ArtifactRule { ecosystem: Ecosystem::Python, target_name: ".pytest_cache", is_exact: true },
    ArtifactRule { ecosystem: Ecosystem::Python, target_name: ".ruff_cache", is_exact: true },
    ArtifactRule { ecosystem: Ecosystem::Python, target_name: ".mypy_cache", is_exact: true },
];

/// Detects project ecosystem from a list of filenames found in a directory.
pub fn detect_ecosystem(folder_files: &[&str]) -> Option<Ecosystem> {
    for file in folder_files {
        let lower = file.to_ascii_lowercase();
        if lower == "cargo.toml" {
            return Some(Ecosystem::Rust);
        }
        if lower == "pubspec.yaml" {
            return Some(Ecosystem::Flutter);
        }
        if lower == "package.json" {
            return Some(Ecosystem::Node);
        }
        if lower.ends_with(".csproj") || lower.ends_with(".fsproj") || lower.ends_with(".sln") {
            return Some(Ecosystem::DotNet);
        }
        if lower == "pyproject.toml"
            || lower == "requirements.txt"
            || lower == "pipfile"
            || lower == "setup.py"
        {
            return Some(Ecosystem::Python);
        }
    }
    None
}

/// Identifies if a directory name is a candidate artifact folder and returns its primary ecosystem.
pub fn is_candidate_name(name: &str) -> Option<Ecosystem> {
    match name {
        "node_modules" | ".next" | ".turbo" | ".nuxt" | "dist" => Some(Ecosystem::Node),
        ".dart_tool" | "ephemeral" => Some(Ecosystem::Flutter),
        "target" => Some(Ecosystem::Rust),
        "bin" | "obj" => Some(Ecosystem::DotNet),
        "__pycache__" | ".venv" | "venv" | ".pytest_cache" | ".ruff_cache" | ".mypy_cache" => {
            Some(Ecosystem::Python)
        }
        "build" => Some(Ecosystem::Node),
        _ => None,
    }
}

/// Checks whether a given directory name matches artifact rules for a specific ecosystem.
pub fn is_candidate_for_ecosystem(name: &str, eco: Ecosystem) -> bool {
    ARTIFACT_RULES
        .iter()
        .any(|rule| rule.ecosystem == eco && rule.target_name.eq_ignore_ascii_case(name))
}

/// Contextual validator for .NET `bin` and `obj` directories.
///
/// Ensures the folder is a direct child of a directory with a `.csproj` or `.fsproj`,
/// AND contains standard compiler output subdirectories (`Debug`, `Release`, or `net*`).
/// Prevents false-positives such as Node CLI packages containing `bin/cli.js`.
pub fn validate_dotnet_bin_obj(path: &Path) -> bool {
    let parent = match path.parent() {
        Some(p) => p,
        None => return false,
    };

    let parent_entries = match std::fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(_) => return false,
    };

    let has_proj = parent_entries.filter_map(Result::ok).any(|entry| {
        let name = entry.file_name();
        let name_str = name.to_string_lossy().to_ascii_lowercase();
        name_str.ends_with(".csproj") || name_str.ends_with(".fsproj")
    });

    if !has_proj {
        return false;
    }

    let bin_entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(_) => return false,
    };

    bin_entries.filter_map(Result::ok).any(|entry| {
        if let Ok(file_type) = entry.file_type() {
            if file_type.is_dir() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy().to_ascii_lowercase();
                return name_str == "debug"
                    || name_str == "release"
                    || name_str.starts_with("net");
            }
        }
        false
    })
}
