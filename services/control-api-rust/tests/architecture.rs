//! Architecture checks.
//!
//! These read the source tree and fail on a violation. The point is that the
//! layer rules survive contact with a deadline: a convention that only lives in
//! a document is a convention until the first urgent change.

use std::fs;
use std::path::{Path, PathBuf};

fn src_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Collect every `.rs` file under a directory.
fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(rust_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    out
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

/// Every folder under `src/` has an `AGENTS.md`.
///
/// This is the mechanical half of "documentation is written during
/// implementation" (ADR 0003): a new folder without its document fails the build
/// before anyone can forget.
#[test]
fn every_source_folder_documents_itself() {
    let root = src_root();
    let mut missing = Vec::new();

    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        if !dir.is_dir() {
            continue;
        }
        let has_rust = rust_files(&dir).iter().any(|path| path.parent() == Some(dir.as_path()));
        if has_rust && !dir.join("AGENTS.md").exists() {
            missing.push(dir.clone());
        }
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            if entry.path().is_dir() {
                stack.push(entry.path());
            }
        }
    }

    assert!(
        missing.is_empty(),
        "these source folders have no AGENTS.md: {missing:#?}"
    );
}

/// The domain layer performs no I/O.
#[test]
fn domain_has_no_io() {
    for file in rust_files(&src_root().join("domain")) {
        let text = read(&file);
        for forbidden in ["sqlx::", "axum::", "reqwest::", "async fn"] {
            assert!(
                !text.contains(forbidden),
                "{} contains '{forbidden}', which does not belong in the domain layer",
                file.display()
            );
        }
    }
}

/// The store layer knows nothing about HTTP.
#[test]
fn store_has_no_http() {
    for file in rust_files(&src_root().join("store")) {
        let text = read(&file);
        for forbidden in ["axum::", "IntoResponse", "StatusCode"] {
            assert!(
                !text.contains(forbidden),
                "{} contains '{forbidden}', which does not belong in the store layer",
                file.display()
            );
        }
    }
}

/// The API layer contains no SQL.
#[test]
fn api_has_no_sql() {
    for file in rust_files(&src_root().join("api")) {
        let text = read(&file);
        for forbidden in ["sqlx::query", "sqlx::query_as", "sqlx::query_scalar"] {
            assert!(
                !text.contains(forbidden),
                "{} contains '{forbidden}'; a handler must call a service",
                file.display()
            );
        }
    }
}

/// The services layer has no HTTP types.
#[test]
fn services_have_no_http_types() {
    for file in rust_files(&src_root().join("services")) {
        let text = read(&file);
        for forbidden in ["axum::Json", "axum::extract", "IntoResponse"] {
            assert!(
                !text.contains(forbidden),
                "{} contains '{forbidden}', which does not belong in a service",
                file.display()
            );
        }
    }
}

/// Production code does not panic on a recoverable path.
///
/// A panic in a handler aborts the task and produces a 500 with no context, which
/// is strictly worse than returning a typed error.
#[test]
fn production_code_does_not_unwrap() {
    let mut offenders = Vec::new();
    for file in rust_files(&src_root()) {
        let text = read(&file);
        // Tests inside modules are allowed to unwrap; the check is on the file
        // before the test module begins.
        let body = text.split("#[cfg(test)]").next().unwrap_or("").to_string();
        for (index, line) in body.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if trimmed.contains(".unwrap()") || trimmed.contains(".expect(") {
                offenders.push(format!("{}:{} {}", file.display(), index + 1, trimmed));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "unwrap/expect outside tests:\n{}",
        offenders.join("\n")
    );
}

/// Every ADR referenced from the index exists.
#[test]
fn adr_index_matches_the_files() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let adr_dir = manifest.join("../../docs/adr");
    if !adr_dir.exists() {
        return;
    }
    let index = read(&adr_dir.join("README.md"));
    for entry in fs::read_dir(&adr_dir).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "README.md" || !name.ends_with(".md") {
            continue;
        }
        assert!(
            index.contains(&name),
            "ADR {name} exists but is not listed in docs/adr/README.md"
        );
    }
}
