fn main() {
    // For WASM builds, we need to configure getrandom properly
    if std::env::var("TARGET")
        .unwrap_or_default()
        .contains("wasm32")
    {
        println!("cargo:rustc-cfg=getrandom_backend=\"js\"");
    }
}
