#!/usr/bin/env bash
# Assemble the source and relink bundle for the statically linked LGPL
# libraries of the Alice Vita release: vitaGL, vitaShaRK, pthread-embedded and
# SceShaccCgExt.
#
# Usage:
#   tools/vita_lgpl_bundle.sh --release-build DIR --capture DIR \
#       --target-root DIR --lib-dir DIR --shacccg-source DIR \
#       --vitagl-source DIR --vitashark-source DIR --pthread-source DIR \
#       [--sdk DIR] --out DIR
#
#   --release-build   output directory of tools/vita_release_build.sh (release mode)
#   --capture         linker wrapper directory holding argv-1.txt and objects/
#   --target-root     Cargo target directory used for the release link
#   --lib-dir         directory that held the pinned libSceShaccCgExt.a
#   --shacccg-source  clean SceShaccCgExt checkout at the pinned revision
#   --vitagl-source   clean vitaGL checkout at the pinned revision
#   --vitashark-source clean vitaShaRK checkout at the pinned revision
#   --pthread-source  clean pthread-embedded checkout at the pinned revision
#   --sdk             Vita SDK holding the release archives (default: $VITASDK)
#   --out             bundle directory to create; the tarball is written next to it
#
# The bundle carries each source tree at the pinned revision, its licence
# files, the exact object code and link command of the release, the release
# link map, a README, a relink script, a packaging script and SHA-256 sums. It
# carries no game data and no binary under another licence. See
# tools/VITA-RELEASE-LIBRARIES.md.
#
# The release build must have used the linker wrapper in
# tools/vita_lgpl_link_capture.sh: rustc deletes the temporary codegen objects
# after a successful link, so the wrapper has to copy them first.
set -euo pipefail

repo=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
exec python3 -I "$repo/tools/vita_lgpl_bundle.py" "$@"
