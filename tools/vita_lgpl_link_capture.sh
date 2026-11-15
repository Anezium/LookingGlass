#!/bin/sh
# Linker wrapper for a release build that must preserve its object code.
#
# LGPL-2.1 section 6 asks for the object code needed to relink the work against
# a modified library. Rust deletes its temporary codegen objects after a
# successful link, so this wrapper copies every target-side input before it
# runs the real linker.
#
# Use it through Cargo's target linker setting, for example:
#   CARGO_TARGET_ARMV7_SONY_VITA_NEWLIBEABIHF_LINKER=$PWD/tools/vita_lgpl_link_capture.sh
# with these variables set:
#   LG_LINK_CAPTURE  empty directory for argv-<n>.txt and objects/
#   LG_REAL_LINKER   the linker to run (absolute path)
#   LG_TARGET_ROOT   Cargo target directory whose files must be preserved
#   LG_VITASDK       Vita SDK root (files there are not copied)
#   LG_LIB_DIR       release library directory (the pinned archive is not copied)
#
# The build must use one link only. tools/vita_lgpl_bundle.sh checks that.
set -eu
: "${LG_LINK_CAPTURE:?}"
: "${LG_REAL_LINKER:?}"
: "${LG_TARGET_ROOT:?}"
: "${LG_VITASDK:?}"
: "${LG_LIB_DIR:?}"

n=0
if [ -f "$LG_LINK_CAPTURE/count" ]; then n=$(cat "$LG_LINK_CAPTURE/count"); fi
n=$((n + 1))
printf '%s\n' "$n" > "$LG_LINK_CAPTURE/count"
argv="$LG_LINK_CAPTURE/argv-$n.txt"
: > "$argv"
for arg in "$@"; do
    printf '%s\n' "$arg" >> "$argv"
done

copy_file() {
    source=$1
    destination=$2
    mkdir -p "$(dirname "$destination")"
    if ! cp -c "$source" "$destination" 2>/dev/null; then
        cp -p "$source" "$destination"
    fi
}

for arg in "$@"; do
    case "$arg" in
        -*|'') continue ;;
    esac
    [ -f "$arg" ] || continue
    case "$arg" in
        "$LG_TARGET_ROOT"/*)
            relative=${arg#"$LG_TARGET_ROOT"/}
            copy_file "$arg" "$LG_LINK_CAPTURE/objects/$relative"
            ;;
        "$LG_VITASDK"/*) ;;
        "$LG_LIB_DIR"/*) ;;
        *)
            printf '%s\n' "$arg" >> "$LG_LINK_CAPTURE/unknown.txt"
            ;;
    esac
done

exec "$LG_REAL_LINKER" "$@"
