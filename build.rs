fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("vita") {
        // Bind persistent GPU binaries to the exact linked vitaGL archive.
        let sdk = std::env::var("VITASDK").expect("VITASDK is required for binary cache identity");
        let archive = std::path::Path::new(&sdk).join("arm-vita-eabi/lib/libvitaGL.a");
        let header = std::path::Path::new(&sdk).join("arm-vita-eabi/include/vitaGL.h");
        println!("cargo:rerun-if-env-changed=VITASDK");
        println!("cargo:rerun-if-changed={}", archive.display());
        println!("cargo:rerun-if-changed={}", header.display());
        let identity = [archive, header].iter().map(|path| {
            let bytes = std::fs::read(path).expect("read vitaGL identity input");
            use sha2::{Digest, Sha256};
            format!("{:x}", Sha256::digest(bytes))
        }).collect::<Vec<_>>().join(":");
        println!("cargo:rustc-env=LG_VITAGL_ID={identity}");
        for symbol in ["malloc", "calloc", "realloc", "vglGetProcAddress", "glTexImage2D", "vglSwapBuffers", "sceCtrlPeekBufferPositive"] {
            println!("cargo:rustc-link-arg=-Wl,--wrap={symbol}");
        }
        // cargo-vita sets RUSTFLAGS, overriding target rustflags in .cargo/config.toml.
        println!("cargo:rustc-link-arg=-Wl,--undefined=sceUserMainThreadStackSize");
        println!("cargo:rustc-link-arg=-Wl,--undefined=_newlib_heap_size_user");
        // Startup tracing and actor diagnostics grow code/exception metadata.
        // Reserve SCE metadata slack (>=3348 bytes),
        // and verify the final program headers for every packaged build.
        println!("cargo:rustc-link-arg=-Wl,--section-start=.init_array=0x82080000");
        if std::env::var_os("CARGO_FEATURE_VITA_RENDERER").is_some() {
            println!("cargo:rustc-link-arg=-Wl,--wrap=invoke_splashscreen");
        }
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
