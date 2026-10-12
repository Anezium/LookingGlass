fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("vita") {
        if std::env::var_os("CARGO_FEATURE_VITA3K_DIAGNOSTICS").is_some() {
            for symbol in ["update_scissor_test", "sceGxmDraw", "sceGxmDrawPrecomputed", "sceGxmFinish", "vglGetProcAddress", "vglSwapBuffers", "sceGxmDisplayQueueAddEntry", "sceGxmEndScene", "sceGxmNotificationWait"] {
                println!("cargo:rustc-link-arg=-Wl,--wrap={symbol}");
            }
        }
        // cargo-vita sets RUSTFLAGS, overriding target rustflags in .cargo/config.toml.
        println!("cargo:rustc-link-arg=-Wl,--undefined=sceUserMainThreadStackSize");
        println!("cargo:rustc-link-arg=-Wl,--undefined=_newlib_heap_size_user");
        // Milestone 3 RX ends near 0x81b9b768; the milestone-2 RW start
        // overlaps exception metadata. Keep SCE metadata slack (>=3348 bytes),
        // and verify the final program headers for every packaged build.
        let init_array = if std::env::var_os("CARGO_FEATURE_VITA3K_DIAGNOSTICS").is_some() { "0x81e00000" } else { "0x81ba0000" };
        println!("cargo:rustc-link-arg=-Wl,--section-start=.init_array={init_array}");
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
