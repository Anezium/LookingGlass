# LiveArea artwork

The doorway, diamond shapes and monoline ALICE lettering are original geometric
artwork for this port. They contain no game image, extracted material, screenshot,
official logo or font. The artwork and generator use this repository's MIT licence.

`python3 tools/vita_livearea.py --generate` regenerates all three palette PNGs
and the template with Pillow11.3.0. This is an optional host packaging dependency,
not a runtime dependency. Install the pinned Pillow11.3.0 in the host Python
environment used by the release script. `python3 tools/vita_livearea.py --vpk path/to/file.vpk`
checks dimensions, indexed palette, image/XML limits, references and exact archive
paths/content. Cargo's static assets root is `vita`, so `sce_sys` stays in the VPK.
The release script checks local assets before compilation and actual packaged
bytes after packing; a missing image, wrong path or changed XML fails the build.

Format references: [VitaSDK samples](https://github.com/vitasdk/samples),
[minimal template](https://github.com/vitasdk/samples/blob/master/hello_world/sce_sys/livearea/contents/template.xml),
and [cargo-vita asset packing](https://github.com/vita-rust/cargo-vita/blob/main/src/commands/build.rs).

Host checks do not prove VitaShell installation or LiveArea presentation. Those
remain an attended console gate on the final release package. This asset-only
branch does not change the embedded game executable or qualify its renderer.
