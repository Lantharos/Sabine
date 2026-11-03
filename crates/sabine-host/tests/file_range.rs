#![cfg(unix)]

use std::{path::Path, process::Command};

#[test]
fn serves_byte_ranges_far_into_large_files() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("file_range");
    std::fs::create_dir_all(&work_dir).unwrap();
    let binary = work_dir.join("file_range");
    let compiler = std::env::var("CXX").unwrap_or_else(|_| "c++".into());
    let compiled = Command::new(compiler)
        .arg("-std=c++20")
        .arg("-I")
        .arg(crate_dir.join("shared"))
        .arg(crate_dir.join("tests/file_range.cc"))
        .arg(crate_dir.join("shared/app/file_range.cc"))
        .arg("-o")
        .arg(&binary)
        .status()
        .unwrap();
    assert!(compiled.success(), "the range reader did not compile");
    let ran = Command::new(&binary).arg(&work_dir).status().unwrap();
    assert!(ran.success(), "ranged reads returned the wrong bytes");
}
