//! Refuse to build with code generated from a different `wit/`.
//!
//! `src/bindings.rs` and `src/design_json.rs` are generated ahead of time
//! (see `sdk/bindgen`) and each records a hash of the WIT it came from. This
//! recomputes the hash, with the same FNV-1a as `bb_sdk_bindgen::wit_hash`,
//! so an edited WIT without regenerated code is a build error rather than
//! silent drift.

use std::path::{Path, PathBuf};

fn wit_hash(dir: &Path) -> u64 {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "wit"))
        .collect();
    files.sort();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for path in files {
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let text = std::fs::read(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
        for byte in name.bytes().chain([0]).chain(text).chain([0]) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    hash
}

fn main() {
    let sdk = Path::new(env!("CARGO_MANIFEST_DIR"));
    let wit = sdk.join("../wit");
    println!("cargo:rerun-if-changed={}", wit.display());
    let expected = format!("// wit-hash: {:016x}", wit_hash(&wit));
    for generated in ["src/bindings.rs", "src/design_json.rs"] {
        let path = sdk.join(generated);
        println!("cargo:rerun-if-changed={}", path.display());
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        if !text.lines().take(3).any(|line| line == expected) {
            panic!(
                "\n\nsdk/{generated} was generated from a different wit/.\n\
                 Regenerate it from the repository root:\n\n    cargo run -p bb-sdk-bindgen\n\n"
            );
        }
    }
}
