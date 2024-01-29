# Benchmark results

All benchmarks are run on a Ryzen 7 5800X, with 32GB of RAM, running Linux 6.6 with `intel_pstate=passive split_lock_detect=off mitigations=off`.

## Results

Coming soon.

## WebAssembly Settings

All WebAssembly files are compiled with the following settings:

- `opt-level` is set to 3, `lto` is set to `thin`, `codegen-units` is set to 1.
- `reference-types`, `bulk-memory`, `mutable-globals` proposals are enabled.

## Runtime Settings

All runtimes are compiled with the following settings:

- `unsafe` features are enabled
- `opt-level` is set to 3, `lto` is set to `thin`, `codegen-units` is set to 1.

## Benchmarking

Benchmarks are run using [Criterion.rs](https://github.com/bheisler/criterion.rs) and can be found in the `benches` directory.

## Running benchmarks

```sh
$ cargo bench --bench <name>
```
