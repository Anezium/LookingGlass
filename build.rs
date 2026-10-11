fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("vita") {
        // cargo-vita sets RUSTFLAGS, overriding target rustflags in .cargo/config.toml.
        println!("cargo:rustc-link-arg=-Wl,--undefined=sceUserMainThreadStackSize");
        println!("cargo:rustc-link-arg=-Wl,--undefined=_newlib_heap_size_user");
        // Profiling ELF: RX ends at 0x81b3fca4; vita-elf-create needs 3348
        // extra bytes for SCE metadata. Move the first RW section one page
        // forward, leaving 4956 bytes. Verify program headers after building.
        println!("cargo:rustc-link-arg=-Wl,--section-start=.init_array=0x81b41000");
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
