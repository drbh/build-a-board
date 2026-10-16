//! The committed bindings must be exactly what the generator produces now.

#[test]
fn bindings_match_the_wit() {
    let (wit, out) = bb_sdk_bindgen::paths();
    let committed = std::fs::read_to_string(&out).expect("sdk/src/bindings.rs exists");
    let fresh = bb_sdk_bindgen::generate(&wit).expect("bindings generate");
    assert!(committed == fresh, "sdk/src/bindings.rs is stale: run `cargo run -p bb-sdk-bindgen`");
}
