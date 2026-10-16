//! `cargo run -p bb-sdk-bindgen`: regenerate the SDK's WIT-derived files from `wit/`.

fn main() -> anyhow::Result<()> {
    for (path, contents) in bb_sdk_bindgen::generate_all(&bb_sdk_bindgen::paths())? {
        std::fs::write(&path, contents)?;
        println!("Wrote {}", path.display());
    }
    Ok(())
}
