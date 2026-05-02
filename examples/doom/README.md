# Doom Example

This example builds a WebAssembly guest from [`PureDOOM`](https://github.com/Daivuk/PureDOOM) and runs it inside `tinywasm`. The host uses `winit` and `softbuffer` to present the framebuffer.

## Prerequisites

- `clang`
- `git`
- a Doom WAD that you provide yourself

## Running the example

From the root of the repository, run:

```sh
# download & build `PureDOOM`
./examples/doom/build.sh

# start doom with the WAD you want to use
cargo doom /path/to/doom.wad
```
