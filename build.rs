//! Compiles the vendored Clipper2 (KiCad 10.0.5's copy) and its C shim.

fn main() {
    let dir = std::path::Path::new("third_party/clipper2");
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .include(dir.join("include"))
        // KiCad builds Clipper2 with Z support; keep the same configuration.
        .define("USINGZ", None)
        .file(dir.join("src/clipper.engine.cpp"))
        .file(dir.join("shim.cpp"))
        .warnings(false)
        .compile("clipper2");
    println!("cargo:rerun-if-changed=third_party/clipper2");
}
