// hashes every .rs file under src/ and exposes the hex digest as the
// TAPE_CODE_HASH environment variable for the main build. recording embeds
// this digest in the trace header; replay refuses to run a trace whose
// code_hash doesn't match the current build.
//
// the assumption: source-file content fully determines the program's
// behaviour. binary-level differences (rustc version, optimization flags)
// don't change which effect calls happen at which sites for the kinds of
// programs we run, so hashing source files instead of the binary keeps the
// hash stable across rebuilds of the same code.

use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

fn main() {
    let mut hasher = Sha256::new();
    let mut paths = Vec::new();
    walk(Path::new("src"), &mut paths);
    paths.sort();

    for p in &paths {
        let bytes = fs::read(p).expect("read source file");
        hasher.update(p.to_string_lossy().as_bytes());
        hasher.update(b"\0");
        hasher.update(&bytes);
        hasher.update(b"\0");
        println!("cargo:rerun-if-changed={}", p.display());
    }

    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    println!("cargo:rustc-env=TAPE_CODE_HASH={hex}");
}

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(p);
        }
    }
}
