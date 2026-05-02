#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

upstream_dir="upstream/PureDOOM"
out_dir="out"
wrapper_src="guest/tinywasm_puredoom.c"
output="$out_dir/puredoom.wasm"

if ! command -v git >/dev/null 2>&1; then
    printf 'missing required tool: git\n' >&2
    exit 1
fi

if ! command -v clang >/dev/null 2>&1; then
    printf 'missing required tool: clang\n' >&2
    exit 1
fi

mkdir -p "upstream" "$out_dir"

if [[ ! -d "$upstream_dir/.git" ]]; then
    git clone --depth 1 https://github.com/Daivuk/PureDOOM.git "$upstream_dir"
else
    git -C "$upstream_dir" pull --ff-only
fi

clang \
    --target=wasm32-unknown-unknown \
    -O2 \
    -nostdlib \
    -fno-builtin \
    -w \
    -Wl,--no-entry \
    -Wl,--allow-undefined \
    -Wl,--export=tinywasm_doom_init \
    -Wl,--export=tinywasm_doom_update \
    -Wl,--export=tinywasm_doom_framebuffer \
    -Wl,--export=tinywasm_doom_sound_buffer \
    -Wl,--export=tinywasm_doom_tick_midi \
    -Wl,--export=tinywasm_doom_wad_path_buf \
    -Wl,--export=tinywasm_doom_key_down \
    -Wl,--export=tinywasm_doom_key_up \
    -Wl,--export=memory \
    -Wl,--export=__heap_base \
    -Wl,--export=__data_end \
    -Wl,--initial-memory=16777216 \
    -Wl,--max-memory=268435456 \
    -Wl,--stack-first \
    -I"$upstream_dir" \
    "$wrapper_src" \
    -o "$output"

printf 'built %s\n' "$output"
