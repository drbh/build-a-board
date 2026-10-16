//! Generates the SDK's code that follows `wit/board.wit`:
//!
//! - `sdk/src/bindings.rs`: the Rust bindings, exactly as
//!   `wit_bindgen::generate!` would expand them inside the SDK;
//! - `sdk/src/design_json.rs`: `ToJson` for every WIT type, so a board built
//!   for the browser can hand its design over as JSON (in serde's default
//!   shape for those types, which `bb-core` reads).
//!
//! The SDK includes the generated file instead of calling the macro, so it
//! compiles without procedural macros (a compiler running as WebAssembly, as
//! in a browser playground, cannot load them). Two guards keep the file in
//! step with the WIT: its header records a hash of `wit/*.wit` that the SDK's
//! build script checks on every build, and this crate's test regenerates the
//! file and compares.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use wit_bindgen_core::WorldGenerator;
use wit_bindgen_core::wit_parser::{Resolve, TypeDefKind};

/// Where the WIT lives and where the SDK's generated files go.
pub struct Paths {
    pub wit: PathBuf,
    pub bindings: PathBuf,
    pub design_json: PathBuf,
}

/// The paths, from this crate's directory.
pub fn paths() -> Paths {
    let sdk = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("sdk/bindgen sits in sdk/").to_path_buf();
    Paths {
        wit: sdk.parent().expect("sdk sits in the repository").join("wit"),
        bindings: sdk.join("src/bindings.rs"),
        design_json: sdk.join("src/design_json.rs"),
    }
}

/// Every generated file: `(path, contents)`.
pub fn generate_all(paths: &Paths) -> Result<Vec<(PathBuf, String)>> {
    Ok(vec![(paths.bindings.clone(), generate(&paths.wit)?), (paths.design_json.clone(), design_json(&paths.wit)?)])
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

fn resolve(wit: &Path) -> Result<(Resolve, wit_bindgen_core::wit_parser::PackageId)> {
    let mut resolve = Resolve::default();
    let (package, _) = resolve.push_path(wit).with_context(|| format!("parsing {}", wit.display()))?;
    Ok((resolve, package))
}

/// The header every generated file starts with.
fn header(wit: &Path) -> Result<String> {
    Ok(format!(
        "// @generated from wit/*.wit by `cargo run -p bb-sdk-bindgen`; do not edit.\n{}\n",
        hash_line(wit_hash(wit)?)
    ))
}

/// The full contents of `bindings.rs` for the WIT in `wit`.
pub fn generate(wit: &Path) -> Result<String> {
    let (resolve, package) = resolve(wit)?;
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
    Ok(format!("{}\n{}", header(wit)?, std::str::from_utf8(source)?))
}

/// `pad-offset` → `PadOffset`, as wit-bindgen names Rust types and cases.
fn camel(name: &str) -> String {
    name.split('-').map(|part| part[..1].to_uppercase() + &part[1..]).collect()
}

/// `hide-reference` → `hide_reference`, as wit-bindgen names fields.
fn snake(name: &str) -> String {
    name.replace('-', "_")
}

/// `ToJson` impls for the named types of the `types` interface, written in
/// serde's default JSON shape for the same Rust types: records as objects
/// keyed by field, enums as `"Case"`, variants as `"Case"` or
/// `{"Case": payload}`.
pub fn design_json(wit: &Path) -> Result<String> {
    let (resolve, package) = resolve(wit)?;
    let interface = resolve.packages[package].interfaces.get("types").context("the WIT has no `types` interface")?;
    let mut out = header(wit)?;
    out.push_str("\n// Included by sdk/src/json.rs, which brings `ToJson` and `key` into scope.\nuse crate::bindings::bb::board::types as w;\n");
    for (name, id) in &resolve.interfaces[*interface].types {
        let ty = camel(name);
        let body = match &resolve.types[*id].kind {
            TypeDefKind::Record(record) => {
                let mut body = String::from("        out.push('{');\n");
                for (i, field) in record.fields.iter().enumerate() {
                    let comma = if i == 0 { "" } else { "," };
                    let field = snake(&field.name);
                    writeln!(body, "        key(out, \"{comma}\", \"{field}\");\n        self.{field}.to_json(out);")?;
                }
                body + "        out.push('}');\n"
            }
            TypeDefKind::Enum(e) => {
                let mut body = String::from("        out.push_str(match self {\n");
                for case in &e.cases {
                    let case = camel(&case.name);
                    writeln!(body, "            w::{ty}::{case} => \"\\\"{case}\\\"\",")?;
                }
                body + "        });\n"
            }
            TypeDefKind::Variant(v) => {
                let mut body = String::from("        match self {\n");
                for case in &v.cases {
                    let case_name = camel(&case.name);
                    if case.ty.is_some() {
                        writeln!(
                            body,
                            "            w::{ty}::{case_name}(value) => {{\n                key(out, \"{{\", \"{case_name}\");\n                value.to_json(out);\n                out.push('}}');\n            }}"
                        )?;
                    } else {
                        writeln!(body, "            w::{ty}::{case_name} => out.push_str(\"\\\"{case_name}\\\"\"),")?;
                    }
                }
                body + "        }\n"
            }
            other => bail!("design_json: unsupported WIT type `{name}` ({other:?})"),
        };
        write!(out, "\nimpl ToJson for w::{ty} {{\n    fn to_json(&self, out: &mut String) {{\n{body}    }}\n}}\n")?;
    }
    Ok(out)
}
