//! `cargo run -p bb-sdk-bindgen`: regenerate `sdk/src/bindings.rs` from `wit/`.

fn main() -> anyhow::Result<()> {
    let (wit, out) = bb_sdk_bindgen::paths();
    std::fs::write(&out, bb_sdk_bindgen::generate(&wit)?)?;
    println!("Wrote {}", out.display());
    Ok(())
}
