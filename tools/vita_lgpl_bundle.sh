#!/usr/bin/env bash
# Assemble the source and relink bundle for the statically linked LGPL
# SceShaccCgExt library of the Alice Vita release.
#
# Usage:
#   tools/vita_lgpl_bundle.sh --release-build DIR --capture DIR \
#       --target-root DIR --lib-dir DIR --shacccg-source DIR --out DIR
#
#   --release-build  output directory of tools/vita_release_build.sh (release mode)
#   --capture        linker wrapper directory holding argv-1.txt and objects/
#   --target-root    Cargo target directory used for the release link
#   --lib-dir        directory that held the pinned libSceShaccCgExt.a
#   --shacccg-source clean SceShaccCgExt checkout at the pinned revision
#   --out            bundle directory to create; the tarball is written next to it
#
# The bundle carries the SceShaccCgExt source at the pinned revision, the
# LGPL-2.1 text, the exact object code and link command of the release, the
# release link map, a README and SHA-256 sums. It carries no game data and no
# binary under another licence. See tools/VITA-RELEASE-LIBRARIES.md.
#
# The release build must have used the linker wrapper in
# tools/vita_lgpl_link_capture.sh: rustc deletes the temporary codegen objects
# after a successful link, so the wrapper has to copy them first.
set -euo pipefail

repo=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
exec python3 -I "$repo/tools/vita_lgpl_bundle.py" "$@"
