fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("vita") {
        // cargo-vita sets RUSTFLAGS, overriding target rustflags in .cargo/config.toml.
        println!("cargo:rustc-link-arg=-Wl,--undefined=sceUserMainThreadStackSize");
        println!("cargo:rustc-link-arg=-Wl,--undefined=_newlib_heap_size_user");
        // Retained-world feature first link: RX ends at 0x81b8747c, beyond
        // the bench base's fixed RW start. Reserve room for SCE metadata
        // (at least 3348 bytes); verify program headers after each build.
        println!("cargo:rustc-link-arg=-Wl,--section-start=.init_array=0x81b90000");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // A level change loads the destination while the viewer and outgoing
        // scene are still live. Windows' 1 MiB default exhausts the main thread
        // stack in that path; reserve 8 MiB, with pages committed on demand.
        // RUST_MIN_STACK only affects spawned threads, not the window thread.
        let arg = if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            "/STACK:8388608"
        } else {
            "-Wl,--stack,8388608"
        };
        println!("cargo:rustc-link-arg={arg}");
    }
}
