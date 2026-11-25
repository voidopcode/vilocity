//! `VILOCITY_SHADER_SOURCE_HASH`: what the translator is built from (this
//! crate's sources, manifest and build script, the workspace's lock file
//! and the compiler's version), for caches of translations
//! (`vilocity_shader::SOURCE_HASH`). `VILOCITY_SHADER_SWITCHES`: every
//! `VILOCITY_*` switch the sources name, whose values join the key at run time.

use std::hash::{DefaultHasher, Hasher};
use std::path::{Path, PathBuf};

fn main() {
    let dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let mut files = vec![
        dir.join("Cargo.toml"),
        dir.join("build.rs"),
        dir.join("../../../Cargo.lock"),
    ];
    sources(&dir.join("src"), &mut files);
    files.sort();
    // SipHash with fixed keys: stable for one compiler, whose version is
    // hashed too.
    let mut h = DefaultHasher::new();
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let version = std::process::Command::new(rustc).arg("-vV").output();
    h.write(&version.map(|o| o.stdout).unwrap_or_default());
    let mut switches = std::collections::BTreeSet::new();
    for f in &files {
        let name = f
            .strip_prefix(&dir)
            .unwrap_or(f)
            .to_string_lossy()
            .replace('\\', "/");
        h.write(name.as_bytes());
        let text = std::fs::read(f).unwrap_or_default();
        h.write(&text);
        if f.starts_with(dir.join("src")) {
            switch_names(&String::from_utf8_lossy(&text), &mut switches);
        }
    }
    for watched in ["src", "Cargo.toml", "build.rs", "../../../Cargo.lock"] {
        println!("cargo:rerun-if-changed={watched}");
    }
    println!(
        "cargo:rustc-env=VILOCITY_SHADER_SOURCE_HASH={:016x}",
        h.finish()
    );
    let joined: Vec<_> = switches.into_iter().collect();
    println!("cargo:rustc-env=VILOCITY_SHADER_SWITCHES={}", joined.join(","));
}

/// The `"VILOCITY_..."` string literals in `text`.
fn switch_names(text: &str, out: &mut std::collections::BTreeSet<String>) {
    let mut rest = text;
    while let Some(at) = rest.find("\"VILOCITY_") {
        let tail = &rest[at + 1..];
        let end = tail
            .find(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
            .unwrap_or(tail.len());
        if tail[end..].starts_with('"') {
            out.insert(tail[..end].to_string());
        }
        rest = &tail[end..];
    }
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else {
            out.push(path);
        }
    }
}
