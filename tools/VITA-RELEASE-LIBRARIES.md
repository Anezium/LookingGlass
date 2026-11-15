# Vita release library selection and LGPL bundle

`vita_release_build.sh --out DIR --lib-dir DIR` is release mode. The directory
must contain the archive pinned by `vita_licenses/SceShaccCgExt-release.json`.
The old SDK v1.0.1 archive is rejected. No library is installed into the SDK by
these tools.

The pin records upstream revision `d3c83efceedeca3be863d0c1f19bb145daf1685f`, its
LGPL-2.1 text hash, and the reviewed archive hash. Export it with `python3 tools/vita_shacccg_no_prelude.py --source UPSTREAM --out NEW_DIR`.
The exporter omits `include/sce_intrinsics.h` and applies the hash-pinned
`patches/SceShaccCgExt-no-prelude.patch` without copying the removed text.
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

## The four LGPL libraries

The release link loads four LGPL libraries. Their pins live in
`vita_licenses/lgpl-libraries.json` (vitaGL, vitaShaRK, pthread-embedded) and
`vita_licenses/SceShaccCgExt-release.json` (SceShaccCgExt). Each pin records the
public source URL, the full revision, the licence, the licence-file hashes, the
release archive hash, the number of archive members, and a hash for every
member the link loaded:

| Library | Revision | Members pinned | Identity |
| --- | --- | ---: | --- |
| vitaGL | `dca4b9d143290d78ec043131a19be36c78cdc7b5` | 30 of 30 | `objcopy --strip-debug` |
| vitaShaRK | `df24065e65098b2d1ac533760109ad4367573f28` (v.1.7) | 1 of 1 | `objcopy --strip-debug` |
| SceShaccCgExt | `d3c83efceedeca3be863d0c1f19bb145daf1685f` | 2 of 2 | raw bytes |
| pthread-embedded | `11d2e5722d98c86f33c908fc47b2cf6e55205db5` | 54 of 134 | `objcopy --strip-debug -R .comment -R .note` |

vitaGL, vitaShaRK and pthread-embedded are compiled with `-g` and the release
link passes `-Wl,--strip-debug`. A rebuild in another directory therefore has a
different archive file hash and different raw member bytes, but its stripped
member code is identical, with one documented exception: vitaGL's
`splashscreen.o` embeds the VitaSDK include path in `.rodata.str1.4` through
`__FILE__`. Its pin is marked `path-dependent`; `relink.sh` requires the member
to be present but does not compare its bytes. VitaSDK also strips every target
object with `--strip-debug -R .comment -R .note`; pthread-embedded's release
archive has already had that pass, and `relink.sh` applies it for that library.
The bundle compares stripped bytes for the other members and raw bytes for
SceShaccCgExt, which keeps the existing release pin exact.

## Source, relink and installation bundle

Every LGPL library needs its source, the object code of the work that uses it,
and enough information to relink and install a modified version. Rust deletes
its temporary codegen objects after a successful link, so the release build has
to run the linker through `tools/vita_lgpl_link_capture.sh`. Set the wrapper as
the target linker and give it a capture directory:

```
export CARGO_TARGET_ARMV7_SONY_VITA_NEWLIBEABIHF_LINKER=$PWD/tools/vita_lgpl_link_capture.sh
export LG_LINK_CAPTURE=/path/to/empty-capture
export LG_REAL_LINKER=$VITASDK/bin/arm-vita-eabi-gcc
export LG_TARGET_ROOT=/path/to/cargo-target
export LG_VITASDK=$VITASDK
export LG_LIB_DIR=/path/to/isolated-libraries
tools/vita_release_build.sh --out /path/to/new-output --lib-dir "$LG_LIB_DIR" \
  --source-bundle alice-lgpl-<head7>-r4.tar.gz
```

Then assemble the source and relink bundle from the release output, the capture,
the four clean source checkouts and the release SDK:

```
tools/vita_lgpl_bundle.sh --release-build /path/to/new-output \
  --capture "$LG_LINK_CAPTURE" --target-root "$LG_TARGET_ROOT" \
  --lib-dir "$LG_LIB_DIR" \
  --shacccg-source /path/to/SceShaccCgExt \
  --vitagl-source /path/to/vitaGL \
  --vitashark-source /path/to/vitaShaRK \
  --pthread-source /path/to/pthread-embedded \
  --sdk "$VITASDK" --out /path/to/alice-lgpl-bundle
```

Every input source checkout must be clean at its pinned revision. The bundle
exporter applies the SceShaccCgExt patch, omits the intrinsic header and ships
the patch separately. The bundle holds the
four source trees, their licence texts, the exact object code and link command,
the release link map, `relink.sh`, `package.sh`, the LiveArea assets and a
`SOURCE-OFFER.txt`; it holds no game data and no binary under another licence.

`relink.sh` accepts a rebuilt archive in `lib/` for any one, several or all four
libraries (`libvitaGL.a`, `libvitashark.a`, `libSceShaccCgExt.a`,
`libpthread.a`). Each supplied archive replaces the SDK one and is checked
against its own pinned members. By default it is a functional relink: a
successful link exits 0 and the script states whether the ELF equals the release
hash. `--strict` (or `LG_RELINK_STRICT=1`) is release reproduction and exits 1
unless the ELF is byte-identical. A link that does not load a supplied archive,
or that also loads the SDK archive of the same name, exits non-zero.

`package.sh` is the LGPL-3.0 section 4(e) installation information: it runs
`vita-elf-create`, `vita-make-fself -s`, `vita-mksfoex` and `vita-pack-vpk` with
the release flags, adding the notices, licence texts and source offer to the
VPK, and prints the ELF, `eboot.bin` and VPK hashes. Installation is copying the
VPK to the Vita and opening it in VitaShell.

## VPK packaging

Release mode calls `tools/vita_package.py` after the native link. It repacks the
VPK with the same commands and flags as the release and requires:

- `THIRD_PARTY_NOTICES-vita.txt`;
- `SOURCE-OFFER.txt` naming the source bundle and the written offer;
- `licenses/GPL-3.0.txt`, `LGPL-3.0.txt`, `LGPL-2.1.txt`,
  `GCC-RUNTIME-EXCEPTION-3.1.txt`, `MPL-2.0.txt`, `MIT.txt`, `Apache-2.0.txt`,
  the newlib BSD texts, and the per-library licence files.

The tool hashes the ELF before and after, checks the VPK's `eboot.bin` and
`param.sfo` byte-for-byte against the release artifacts, and fails closed on a
missing notice or licence. Packaging is the only change: no runtime source or
link input changes, so the ELF stays identical.

The MPL-2.0 source route is stated in the notices and the source offer: the four
symphonia 0.5.5 crates are unmodified, their source is the crates.io archive,
and the Cargo.lock SHA-256 values are printed.

These checks close the library-selection, source/relink and package-notice
gates. Installation and device behavior remain separate release gates.
