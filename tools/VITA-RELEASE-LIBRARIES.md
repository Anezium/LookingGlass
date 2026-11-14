# Vita release library selection

`vita_release_build.sh --out DIR --lib-dir DIR` is release mode. The directory
must contain the archive pinned by `vita_licenses/SceShaccCgExt-release.json`.
The old SDK v1.0.1 archive is rejected. No library is installed into the SDK by
these tools.

The pin records upstream revision `d3c83efceedeca3be863d0c1f19bb145daf1685f`, its
LGPL-2.1 text hash, and the reviewed archive hash. Build that revision outside
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

These checks close this library-selection gate. Installation, device behavior,
the other library/source obligations and the remaining release gates are
separate work.
