//! Generates `sdk/src/bindings.rs`: the Rust bindings for `wit/board.wit`,
//! exactly as `wit_bindgen::generate!` would expand them inside the SDK.
//!
//! The SDK includes the generated file instead of calling the macro, so it
//! compiles without procedural macros (a compiler running as WebAssembly, as
//! in a browser playground, cannot load them). Two guards keep the file in
//! step with the WIT: its header records a hash of `wit/*.wit` that the SDK's
//! build script checks on every build, and this crate's test regenerates the
//! file and compares.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use wit_bindgen_core::WorldGenerator;
use wit_bindgen_core::wit_parser::Resolve;

/// Where the WIT lives and the bindings go, from this crate's directory.
pub fn paths() -> (PathBuf, PathBuf) {
    let sdk = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("sdk/bindgen sits in sdk/").to_path_buf();
    (sdk.parent().expect("sdk sits in the repository").join("wit"), sdk.join("src/bindings.rs"))
}

/// FNV-1a over every `.wit` file in `dir`, names included, in name order.
/// The SDK's build script computes the same hash (keep the two in step).
pub fn wit_hash(dir: &Path) -> Result<u64> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "wit"))
        .collect();
    files.sort();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for path in files {
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let text = std::fs::read(&path)?;
        for byte in name.bytes().chain([0]).chain(text).chain([0]) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    Ok(hash)
}

/// The header line carrying the hash.
pub fn hash_line(hash: u64) -> String {
    format!("// wit-hash: {hash:016x}")
}

/// The full contents of `bindings.rs` for the WIT in `wit`.
pub fn generate(wit: &Path) -> Result<String> {
    let mut resolve = Resolve::default();
    let (package, _) = resolve.push_path(wit).with_context(|| format!("parsing {}", wit.display()))?;
    let world = resolve.select_world(&[package], Some("board"))?;
    // The options `generate!` was called with in sdk/src/wit.rs.
    let opts = wit_bindgen_rust::Opts {
        pub_export_macro: true,
        default_bindings_module: Some("bb_sdk::bindings".into()),
        format: true,
        ..Default::default()
    };
    let mut files = Default::default();
    opts.build().generate(&resolve, world, &mut files)?;
    let Some((_, source)) = files.iter().next() else { bail!("wit-bindgen produced no output") };
    Ok(format!(
        "// @generated from wit/*.wit by `cargo run -p bb-sdk-bindgen`; do not edit.\n{}\n\n{}",
        hash_line(wit_hash(wit)?),
        std::str::from_utf8(source)?
    ))
}
