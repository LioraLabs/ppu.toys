// Loading a program parses it with full_moon, which recurses once per nested block: wasm32's
// default 1 MiB stack overflows near 90 levels, and the trap leaves the module unusable until
// a reload. Native threads already have 8 MiB.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        println!("cargo:rustc-link-arg=-zstack-size=8388608");
    }
}
