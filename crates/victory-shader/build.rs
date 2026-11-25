//! Stable fingerprint for translated shader output. Unlike hashing the
//! executable, this changes only when the translator implementation or its
//! build configuration changes.
use std::path::{Path, PathBuf};

fn collect(dir: &Path, root: &Path, files: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("read translator source directory")
        .map(|entry| entry.expect("read translator source entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, root, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            println!("cargo:rerun-if-changed={}", path.display());
            files.push(path.strip_prefix(root).unwrap().to_path_buf());
        }
    }
}

fn main() {
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let mut files = Vec::new();
    collect(&root.join("src"), &root, &mut files);
    files.push(PathBuf::from("Cargo.toml"));
    files.push(PathBuf::from("build.rs"));

    let mut hash = blake3::Hasher::new();
    for path in files {
        let name = path.to_string_lossy().replace('\\\\', "/");
        let full = root.join(&path);
        println!("cargo:rerun-if-changed={}", full.display());
        hash.update(name.as_bytes());
        hash.update(&[0]);
        hash.update(&std::fs::read(full).expect("read translator fingerprint input"));
        hash.update(&[0]);
    }

    let target = std::env::var("TARGET").unwrap_or_default();
    let diagnostics = std::env::var_os("CARGO_FEATURE_DIAGNOSTICS").is_some();
    hash.update(target.as_bytes());
    hash.update(&[u8::from(diagnostics)]);
    hash.update(b"translated-host-shader-cache-v2");

    println!(
        "cargo:rustc-env=VICTORY_SHADER_TRANSLATOR_FINGERPRINT={}",
        hash.finalize().to_hex()
    );
}
