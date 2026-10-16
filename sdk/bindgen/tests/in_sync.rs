//! The committed generated files must be exactly what the generator produces now.

#[test]
fn generated_files_match_the_wit() {
    for (path, fresh) in bb_sdk_bindgen::generate_all(&bb_sdk_bindgen::paths()).expect("files generate") {
        let committed = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(committed == fresh, "{} is stale: run `cargo run -p bb-sdk-bindgen`", path.display());
    }
}
