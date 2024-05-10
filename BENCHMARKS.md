# Benchmark results

All benchmarks are run on a Ryzen 7 5800X with 32GB of RAM on Linux 6.6.
WebAssembly files are optimized using [wasm-opt](https://github.com/WebAssembly/binaryen),
and the benchmark code is available in the `crates/benchmarks` folder.

These are mainly preliminary benchmarks, and I will be rewriting the benchmarks to be more accurate and to test more features in the future.
In particular, I want to test and improve memory usage, as well as the performance of the parser.

Take these results with a grain of salt, as they are not very accurate and are likely to change in the future.

## WebAssembly Settings

All WebAssembly files are compiled with the following settings:

- `opt-level` is set to 3, `lto` is set to `thin`, `codegen-units` is set to 1.
- `reference-types`, `bulk-memory`, `mutable-globals` proposals are enabled.

## Runtime Settings

All runtimes are compiled with the following settings:

- `unsafe` features are enabled.
- `opt-level` is set to 3, `lto` is set to `thin`, `codegen-units` is set to 1.
- No CPU-specific optimizations are used as AVX2 can reduce performance by more than 50% on some CPUs.

## Versions

- `tinywasm`: `0.6.2`
- `wasmi`: `0.31.2`
- `wasmer`: `4.2.8`

## Results

| Benchmark    | Native   | TinyWasm   | Wasmi     | Wasmer (Single Pass) |
| ------------ | -------- | ---------- | --------- | -------------------- |
| `fib`        | `0ms`    | ` 19.09µs` | `18.53µs` | ` 48.09µs`           |
| `fib-rec`    | `0.27ms` | ` 22.22ms` | ` 4.96ms` | `  0.47ms`           |
| `argon2id`   | `0.53ms` | ` 86.42ms` | `46.36ms` | `  4.82ms`           |
| `selfhosted` | `0.05ms` | `  7.26ms` | ` 6.51ms` | `446.48ms`           |

### Fib

The first benchmark is a simple optimized Fibonacci function, a good way to show the overhead of calling functions and parsing the bytecode.
TinyWasm is slightly faster than Wasmi here, but that's probably because of the overhead of parsing the bytecode, as TinyWasm uses a custom bytecode to pre-process the WebAssembly bytecode.

### Fib-Rec

This benchmark is a recursive Fibonacci function, highlighting some issues with the current implementation of TinyWasm's Call Stack.
TinyWasm is a lot slower here, but that's because there's currently no way to reuse the same Call Frame for recursive calls, so a new Call Frame is allocated for every call. This is not a problem for most programs; the upcoming `tail-call` proposal will make this much easier to implement.

### Argon2id

This benchmark runs the Argon2id hashing algorithm with 2 iterations, 1KB of memory, and 1 parallel lane.
I had to decrease the memory usage from the default to 1KB because the interpreters were struggling to finish in a reasonable amount of time.
This is where `simd` instructions would be really useful, and it also highlights some of the issues with the current implementation of TinyWasm's Value Stack and Memory Instances. These spend much time on stack operations, so they might be a good place to experiment with Arena Allocation.

### Selfhosted

This benchmark runs TinyWasm itself in the VM, and parses and executes the `print.wasm` example from the `examples` folder.
This is a good way to show some of TinyWasm's strengths - the code is quite large at 702KB and Wasmer struggles massively with it, even with the Single Pass compiler. I think it's a decent real-world performance benchmark, but it definitely favors TinyWasm a bit.

Wasmer also offers a pre-parsed module format, so keep in mind that this number could be a bit lower if that was used (but probably still on the same order of magnitude). This number seems so high that I'm not sure if I'm doing something wrong, so I will be looking into this in the future.

### Conclusion

After profiling and fixing some low-hanging fruits, I found the biggest bottleneck to be Vector operations, especially for the Value Stack, and having shared access to Memory Instances using RefCell. These are the two areas I will focus on improving in the future, trying out Arena Allocation and other data structures to improve performance. Additionally, typed FuncHandles have a significant overhead over the untyped ones, so I will also look into improving that. Still, I'm pretty happy with the results, especially considering the focus on simplicity and portability over performance.

Something that made a much more significant difference than I expected was to give compiler hints about cold paths and to force the inlining of some functions. This made the benchmarks 30%+ faster in some cases. Many places in the codebase have comments about what optimizations have been done.

# Running benchmarks

Benchmarks are run using [Criterion.rs](https://github.com/bheisler/criterion.rs). To run a benchmark, use the following command:

```sh
$ cargo benchmark <name>
```

# Profiling

To profile a benchmark, use the following command:

```sh
$ cargo flamegraph -p benchmarks --bench <name> -- --bench
```

This will generate a flamegraph in `flamegraph.svg` and a `perf.data` file.
You can use [hotspot](https://github.com/KDAB/hotspot) to analyze the `perf.data` file.
Since a lot of functions are inlined, you probably want to remove the `#[inline]` attribute from the functions you care about.
Note that this will make the benchmark considerably slower, 2-10x slower in some cases.
