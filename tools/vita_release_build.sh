#!/usr/bin/env bash
# Release build of the PS Vita SELF and VPK.
#
# Usage: tools/vita_release_build.sh --out DIR [--lib-dir DIR]
#
#   --out DIR      new or empty directory for the artifacts and BUILD.txt
#   --lib-dir DIR  directory searched before the SDK for the C archives the link pulls in,
#                  for example a libSceShaccCgExt.a rebuilt at its LGPL-2.1 revision
#
# Needs VITASDK, the pinned Rust toolchain with rust-src, and cargo-vita. Refuses a tree with
# uncommitted or untracked files. Honours CARGO_TARGET_DIR, CARGO_BUILD_JOBS and
# CARGO_NET_OFFLINE from the caller. Prints the SHA-256 of the ELF, SELF and VPK.
set -euo pipefail

TOOLCHAIN=nightly-2026-10-11
CARGO_VITA_VERSION=0.2.2
FEATURES=native_timing,vita_renderer,frame_total_only
HEAP_BYTES=$((192 * 1024 * 1024))
APP_VER=01.00
# vdpm package and archive name of each C library linked statically.
SDK_LIBRARIES="vitaGL:libvitaGL.a vitaShaRK:libvitashark.a SceShaccCgExt:libSceShaccCgExt.a libmathneon:libmathneon.a"

out=""
lib_dir=""
while [ $# -gt 0 ]; do
    case "$1" in
        --out) out="$2"; shift 2 ;;
        --lib-dir) lib_dir="$2"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
[ -n "$out" ] || { echo "usage: $0 --out DIR [--lib-dir DIR]" >&2; exit 2; }
: "${VITASDK:?VITASDK must point to the Vita SDK}"

repo=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
cd "$repo"
if [ -n "$(git status --porcelain=v1 --untracked-files=all)" ]; then
    echo "refusing to build: the tree has uncommitted or untracked files" >&2
    exit 1
fi
head=$(git rev-parse HEAD)

mkdir -p "$out"
out=$(cd "$out" && pwd)
[ -z "$(ls -A "$out")" ] || { echo "refusing to build: $out is not empty" >&2; exit 1; }
if [ -n "$lib_dir" ]; then
    lib_dir=$(cd "$lib_dir" && pwd)
fi

rustup run "$TOOLCHAIN" rustc --version >/dev/null
rustup component list --toolchain "$TOOLCHAIN" --installed | grep -q '^rust-src' \
    || { echo "rust-src is missing for $TOOLCHAIN" >&2; exit 1; }
cargo install --list | grep -q "^cargo-vita v$CARGO_VITA_VERSION:" \
    || { echo "cargo-vita $CARGO_VITA_VERSION is required" >&2; exit 1; }

# Path remapping keeps the build machine's directories out of panic locations.
# rustc applies the last matching prefix, so the more specific ones come after HOME.
cargo_home=${CARGO_HOME:-$HOME/.cargo}
rustup_home=${RUSTUP_HOME:-$HOME/.rustup}
rustflags="-C target-feature=-neon"
rustflags="$rustflags --remap-path-prefix=$HOME=/home"
rustflags="$rustflags --remap-path-prefix=$cargo_home=/cargo"
rustflags="$rustflags --remap-path-prefix=$rustup_home=/rustup"
rustflags="$rustflags --remap-path-prefix=$repo=/build/looking-glass"

export RUSTUP_TOOLCHAIN="$TOOLCHAIN"
export RUSTFLAGS="$rustflags"
export CARGO_INCREMENTAL=0
export PATH="$VITASDK/bin:$PATH"
export LG_VITA_LINK_MAP="$out/link.map"
if [ -n "$lib_dir" ]; then
    export LG_VITA_LIB_DIR="$lib_dir"
else
    unset LG_VITA_LIB_DIR
fi
target_dir=${CARGO_TARGET_DIR:-$repo/target}
release="$target_dir/armv7-sony-vita-newlibeabihf/release"
rm -f "$release/looking-glass.elf" "$release/looking-glass.velf" "$release/looking-glass.self" \
      "$release/looking-glass.sfo" "$release/looking-glass.vpk"

start=$(date +%s)
cargo vita build vpk --release --locked --features "$FEATURES" 2>&1 | tee "$out/build.log"
seconds=$(( $(date +%s) - start ))

[ "$(git rev-parse HEAD)" = "$head" ] || { echo "HEAD moved during the build" >&2; exit 1; }
[ -z "$(git status --porcelain=v1 --untracked-files=all)" ] || { echo "the tree changed during the build" >&2; exit 1; }

cp "$release/looking-glass.elf" "$out/looking-glass.elf"
cp "$release/looking-glass.self" "$out/eboot.bin"
cp "$release/looking-glass.sfo" "$out/param.sfo"
cp "$release/looking-glass.vpk" "$out/looking-glass.vpk"

python3 -I "$repo/tools/vita_release_check.py" \
    --elf "$out/looking-glass.elf" --self "$out/eboot.bin" --sfo "$out/param.sfo" --vpk "$out/looking-glass.vpk" \
    --heap "$HEAP_BYTES" --forbid "$HOME" --sfo-field "APP_VER=$APP_VER" --sfo-field TITLE_ID=ALICELG01 \
    | tee "$out/check.txt" || true
check_status=${PIPESTATUS[0]}

redact() { sed "s|$HOME|~|g"; }
pacman_db="$VITASDK/var/lib/pacman/local"
{
    echo "head $head"
    echo "seconds $seconds"
    echo "toolchain $(rustup run "$TOOLCHAIN" rustc --version)"
    echo "cargo-vita $CARGO_VITA_VERSION"
    echo "features $FEATURES"
    echo "rustflags $RUSTFLAGS" | redact
    echo "lib-dir ${lib_dir:-none}" | redact
    grep -A2 'source = "git+' Cargo.lock | grep '^source' | sed 's/^source = /dependency /'
    echo "sdk-build $(head -n 1 "$VITASDK/version_info.txt" 2>/dev/null || echo unknown)"
    sed -n '2,$p' "$VITASDK/version_info.txt" 2>/dev/null | sed 's/^/sdk-component /'
    # For each library: the vdpm package installed, the archive the linker actually loaded
    # (from the link map), its hash, and whether it is the file that package installed.
    for pair in $SDK_LIBRARIES; do
        package=${pair%%:*}
        archive=${pair#*:}
        installed=$(ls "$pacman_db" 2>/dev/null | grep "^$package-[0-9]" || true)
        path=$(grep -o "[^ (]*/$archive" "$out/link.map" | sort -u | head -n 1 || true)
        if [ -z "$path" ]; then
            echo "linked $archive not-in-link-map package=${installed:-none}"
            continue
        fi
        sha=$(shasum -a 256 "$path" | cut -d' ' -f1)
        packaged=""
        if [ -n "$installed" ]; then
            packaged=$(gzip -dc "$pacman_db/$installed/mtree" | grep "lib/$archive " | grep -o 'sha256digest=[0-9a-f]*' | cut -d= -f2 || true)
        fi
        if [ "$sha" = "$packaged" ]; then origin=same-as-package; else origin=not-the-package-file; fi
        echo "linked $archive sha256=$sha package=${installed:-none} $origin path=$path" | redact
    done
    echo "vitagl-recipe make NO_TEX_COMBINER=1 HAVE_UNFLIPPED_FBOS=1 SINGLE_THREADED_GC=1 HAVE_CUSTOM_HEAP=1 (documented; the archive does not record its flags)"
    cd "$out"
    shasum -a 256 looking-glass.elf eboot.bin looking-glass.vpk
    wc -c eboot.bin looking-glass.vpk
} | tee "$out/BUILD.txt"
# The link map lists absolute archive paths; keep the record, drop the home directory.
sed -i.bak "s|$HOME|~|g" "$out/link.map" && rm -f "$out/link.map.bak"

# Notices for this exact link. The committed file must match, or the release ships stale notices.
notices_status=0
python3 -I "$repo/tools/vita_notices.py" --link-map "$out/link.map" --output "$out/THIRD_PARTY_NOTICES-vita.txt" \
    || notices_status=$?
if [ "$notices_status" -ne 0 ]; then
    echo "FAIL notices generation" | tee -a "$out/check.txt"
elif cmp -s "$out/THIRD_PARTY_NOTICES-vita.txt" "$repo/THIRD_PARTY_NOTICES-vita.txt"; then
    echo "PASS THIRD_PARTY_NOTICES-vita.txt matches this link" | tee -a "$out/check.txt"
else
    echo "FAIL THIRD_PARTY_NOTICES-vita.txt differs from the notices generated for this link" | tee -a "$out/check.txt"
fi
if grep -q "linked libSceShaccCgExt.a .*package=SceShaccCgExt-1.0.1-1 same-as-package" "$out/BUILD.txt"; then
    echo "WARN libSceShaccCgExt.a is the SDK's v1.0.1 package (GPL-3.0); release builds pass --lib-dir with a d3c83ef or later build" \
        | tee -a "$out/check.txt"
fi

if [ "$check_status" -ne 0 ] || grep -q '^FAIL' "$out/check.txt"; then
    echo "release checks failed" >&2
    exit 1
fi
echo "release build done: $out"
