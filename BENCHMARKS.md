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

# Running benchmarks

Benchmarks are run using [Criterion.rs](https://github.com/bheisler/criterion.rs). To run a benchmark, use the following command:

```sh
$ cargo bench --bench <name>
```

## Profiling

To profile a benchmark, use the following command:

```sh
$ cargo flamegraph --bench <name> -- --bench
```

This will generate a flamegraph in `flamegraph.svg` and a `perf.data` file.
You can use [hotspot](https://github.com/KDAB/hotspot) to analyze the `perf.data` file.
Since a lot of functions are inlined, you probably want to remove the `#[inline]` attribute from the functions you care about.
Note that this will make the benchmark considerably slower, 2-10x slower in some cases.
