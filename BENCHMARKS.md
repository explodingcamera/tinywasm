# Benchmark results

All benchmarks are run on a Ryzen 7 5800X, with 32GB of RAM, running Linux 6.6.
WebAssembly files are optimized using [wasm-opt](https://github.com/WebAssembly/binaryen)
and the benchmark code is available in the `benches` folder.

## WebAssembly Settings

All WebAssembly files are compiled with the following settings:

- `opt-level` is set to 3, `lto` is set to `thin`, `codegen-units` is set to 1.
- `reference-types`, `bulk-memory`, `mutable-globals` proposals are enabled.

## Runtime Settings

All runtimes are compiled with the following settings:

- `unsafe` features are enabled
- `opt-level` is set to 3, `lto` is set to `thin`, `codegen-units` is set to 1.

## Results

| Benchmark    | Native | TinyWasm | Wasmi    | Wasmer (Single Pass) |
| ------------ | ------ | -------- | -------- | -------------------- |
| `argon2id`   | 0.52ms | 110.08ms | 44.408ms | 4.76ms               |
| `fib`        | 6ns    | 44.76µs  | 48.96µs  | 52µs                 |
| `fib-rec`    | 284ns  | 25.565ms | 5.11ms   | 0.50ms               |
| `selfhosted` | 45µs   | 2.18ms   | 4.25ms   | 258.87ms             |

### Argon2id

This benchmark runs the Argon2id hashing algorithm, with 2 iterations, 1KB of memory, and 1 parallel lane.
I had to decrease the memory usage from the default to 1KB, because especially the interpreters were struggling to finish in a reasonable amount of time.
This is something where `simd` instructions would be really useful, and it also highlights some of the issues with the current implementation of TinyWasm's Value Stack and Memory Instances.

### Fib

The first benchmark is a simple optimized Fibonacci function, which is a good way to show the overhead of calling functions and parsing the bytecode.
TinyWasm is slightly faster then Wasmi here, but that's probably because of the overhead of parsing the bytecode as TinyWasm uses a custom bytecode to pre-process the WebAssembly bytecode.

### Fib-Rec

This benchmark is a recursive Fibonacci function, which highlights some of the issues with the current implementation of TinyWasm's Call Stack.
TinyWasm is a lot slower here, but that's because there's currently no way to reuse the same Call Frame for recursive calls, so a new Call Frame is allocated for every call. This is not a problem for most programs, and the upcoming `tail-call` proposal will make this a lot easier to implement.

### Selfhosted

This benchmark runs TinyWasm itself in the VM, and parses and executes the `print.wasm` example from the `examples` folder.
This is a godd way to show some of TinyWasm's strengths - the code is pretty large at 702KB and Wasmer struggles massively with it, even with the Single Pass compiler. I think it's a decent real-world performance benchmark, but definitely favors TinyWasm a bit.

Wasmer also offers a pre-parsed module format, so keep in mind that this number could be a bit lower if that was used (but probably still on the same order of magnitude). This number seems so high that I'm not sure if I'm doing something wrong, so I will be looking into this in the future.

### Conclusion

After profiling and fixing some low hanging fruits, I found the biggest bottleneck to be Vector operations, especially for the Value Stack, and having shared access to Memory Instances using RefCell. These are the two areas I will be focusing on improving in the future, trying out to use
Arena Allocation and other data structures to improve performance. Still, I'm quite happy with the results, especially considering the use of standard Rust data structures. Additionally, typed FuncHandles have a significant overhead over the untyped ones, so I will be looking into improving that as well.

# Running benchmarks

Benchmarks are run using [Criterion.rs](https://github.com/bheisler/criterion.rs). To run a benchmark, use the following command:

```sh
$ cargo bench --bench <name>
```

# Profiling

To profile a benchmark, use the following command:

```sh
$ cargo flamegraph --bench <name> -- --bench
```

This will generate a flamegraph in `flamegraph.svg` and a `perf.data` file.
You can use [hotspot](https://github.com/KDAB/hotspot) to analyze the `perf.data` file.
Since a lot of functions are inlined, you probably want to remove the `#[inline]` attribute from the functions you care about.
Note that this will make the benchmark considerably slower, 2-10x slower in some cases.
