#!/bin/sh
# Thin launcher for the standalone Rust application; no runtime setup or build.
set -eu
case $0 in
    */*) script_path=${0%/*} ;;
    *) script_path=. ;;
esac
script_dir=$(CDPATH= cd -- "$script_path" && pwd -P)
binary=
for candidate in "$script_dir/numera-emoji" "$script_dir/runtime/target/release/numera-emoji" "$script_dir/runtime/target/debug/numera-emoji"; do
    if [ -L "$candidate" ]; then
        echo 'Refusing a linked standalone executable.' >&2
        exit 1
    fi
    if [ -f "$candidate" ] && [ -x "$candidate" ]; then
        binary=$candidate
        break
    fi
done
if [ -z "$binary" ]; then
    echo 'Installed native executable missing. Extract a standalone package, or explicitly build with cargo build --manifest-path runtime/Cargo.toml --locked --release.' >&2
    exit 1
fi
if [ "$#" -eq 0 ]; then
    set -- menu
elif [ "$1" = '--check' ]; then
    shift
    if [ "$#" -ne 0 ]; then
        echo '--check accepts no command arguments.' >&2
        exit 2
    fi
    set -- doctor
fi
cd -- "$script_dir"
exec "$binary" "$@"
