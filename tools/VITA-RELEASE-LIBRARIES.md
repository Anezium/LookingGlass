# Vita release library selection

`vita_release_build.sh --out DIR --lib-dir DIR` is release mode. The directory
must contain the archive pinned by `vita_licenses/SceShaccCgExt-release.json`.
The old SDK v1.0.1 archive is rejected. No library is installed into the SDK by
these tools.

The pin records upstream revision `d3c83efceedeca3be863d0c1f19bb145daf1685f`, its
LGPL-2.1 text hash, and the reviewed archive hash. The release applies `patches/SceShaccCgExt-no-prelude.patch` and omits
`include/sce_intrinsics.h`. Export the patched source with
`python3 tools/vita_shacccg_no_prelude.py --source UPSTREAM --out NEW_DIR`.
The exporter verifies the upstream revision and never copies the removed header.
Build that exported source outside
this checkout with the matching Vita SDK/toolchain, for example using CMake with
`-DCMAKE_POLICY_VERSION_MINIMUM=3.5` and the current Vita packaging flags:

```
-DCMAKE_C_FLAGS="-std=gnu11 -Wno-error=implicit-function-declaration -Wno-error=int-conversion -Wno-error=incompatible-pointer-types"
-DCMAKE_CXX_FLAGS="-std=gnu++11 -Wno-error=implicit-function-declaration -Wno-error=int-conversion -Wno-error=incompatible-pointer-types"
```

Copy only the resulting `libSceShaccCgExt.a` into an isolated library directory.
The preflight is read-only and can be run before an expensive native build:

```
tools/vita_release_build.sh --check-libraries --lib-dir /path/to/isolated-libraries
tools/vita_release_build.sh --out /path/to/new-output --lib-dir /path/to/isolated-libraries
```

After Cargo links, the script checks every SceShaccCgExt archive path in the
actual link map, requires contributing members from the selected archive, and
rechecks the archive bytes. The generated `library-selection.json` records the
source, licence, archive identity, actual member names and individual hashes,
and the final redacted link-map hash. Generated notices must match the committed
notices for that link. A linker search option alone is not sufficient evidence.

To relink against a modified library or a new compiler build, review the source,
licence and resulting archive and update the explicit source/archive pin in your
source copy. The old SDK archive does not become a release input by renaming it.

For general local compilation checks, explicitly choose
`--private-test --out DIR [--lib-dir DIR]`. This permits SDK fallback, labels the
result in `BUILD.txt` and `PRIVATE-TEST.txt`, and does not generate qualified
release notices. ELF/package/path-remapping checks still apply. It is not a
release qualification shortcut.

## Source and relink bundle

LGPL-2.1 section 6 also asks for the library source and the object code of the
work so that a user can relink against a modified library. Rust deletes its
temporary codegen objects after a successful link, so the release build has to
run the linker through `tools/vita_lgpl_link_capture.sh`. Set the wrapper as
the target linker and give it a capture directory:

```
export CARGO_TARGET_ARMV7_SONY_VITA_NEWLIBEABIHF_LINKER=$PWD/tools/vita_lgpl_link_capture.sh
export LG_LINK_CAPTURE=/path/to/empty-capture
export LG_REAL_LINKER=$VITASDK/bin/arm-vita-eabi-gcc
export LG_TARGET_ROOT=/path/to/cargo-target
export LG_VITASDK=$VITASDK
export LG_LIB_DIR=/path/to/isolated-libraries
tools/vita_release_build.sh --out /path/to/new-output --lib-dir "$LG_LIB_DIR"
```

Then assemble the source and relink bundle from the release output and the
capture:

```
tools/vita_lgpl_bundle.sh --release-build /path/to/new-output \
  --capture "$LG_LINK_CAPTURE" --target-root "$LG_TARGET_ROOT" \
  --lib-dir "$LG_LIB_DIR" --shacccg-source /path/to/SceShaccCgExt \
  --out /path/to/alice-lgpl-bundle
```

The input source checkout must be clean at the pinned revision. The bundle exporter
applies the same patch, omits the intrinsic header and ships the patch separately. The bundle holds the
exact object code and link command, the source, the licence text, the release
link map and a relink script; it holds no game data and no binary under another
licence. The relink script has two modes. By default it is a functional relink:
a successful link exits 0 and the script states whether the ELF equals the
release hash. `--strict` (or `LG_RELINK_STRICT=1`) is release reproduction and
exits 1 unless the ELF is byte-identical. The generated README carries the
public rebuild recipe for the vdpm packages and the custom vitaGL archive.

These checks close this library-selection gate. Installation, device behavior,
the other library/source obligations and the remaining release gates are
separate work.
