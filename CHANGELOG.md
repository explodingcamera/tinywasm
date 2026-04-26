# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

This release is a major runtime and API rework. It adds support for several newer WebAssembly proposals, introduces the new `Engine` configuration API, rewrites large parts of execution and validation, and changes the internal `twasm` archive format. Benchmarks in the repository currently show roughly 30-90% improvement over 0.8.0 depending on workload and execution mode.

### Added

- Support for the `custom_page_sizes` proposal ([#22](https://github.com/explodingcamera/tinywasm/pull/22) by [@danielstuart14](https://github.com/danielstuart14))
- Support for the `tail_call`, `memory64`, `simd`, `relaxed_simd`, `wide_arithmetic`, and `extended_const` proposals ([#37](https://github.com/explodingcamera/tinywasm/pull/37), [#38](https://github.com/explodingcamera/tinywasm/pull/38), [#39](https://github.com/explodingcamera/tinywasm/pull/39))
- Parse-only support for the `annotations` proposal
- New `Engine` API (`tinywasm::Engine` and `engine::Config`) for runtime configuration
- Resumable execution APIs: `call_resumable`, `resume_with_fuel`, `resume_with_time_budget`, and `ExecProgress`
- Host-function fuel APIs: `FuncContext::charge_fuel` and `FuncContext::remaining_fuel`
- `engine::Config` support for fuel policy, stack sizing, memory backend selection, and trap-on-OOM behavior
- New feature flags: `canonicalize-nans`, `simd-x86`, `guest-debug`, `debug`, and `parallel-parser`
- Top-level parser re-exports behind the `parser` feature: `parse_bytes`, `parse_file`, and `parse_stream`

### Changed

- `Store::new` now takes an `Engine`; use `Store::default()` for default settings.
- `ModuleInstance::func` now validates exact Wasm signatures at lookup time and fails immediately on mismatches.
- Stack and call-stack limits now come from `engine::Config`, and memory allocation is lazy until first access.
- Module-internal by-index inspection APIs are now gated behind `guest-debug`, and runtime `Debug` implementations are gated behind `debug`.
- `Module` is now re-exported directly from `tinywasm_types`; the `module` submodule was removed.
- MSRV increased to 1.95 and the crate now uses Rust 2024.
- `Error`, `Trap`, and `LinkingError` are now `#[non_exhaustive]`.
- `Trap` variant discriminants changed; do not rely on casting variants to integers.
- `HostFunction::new`, `HostFunction::func`, and `HostFunction::typed` now require `&mut Store`, and `Imports::link_module` now takes a `ModuleInstance` instead of a raw module instance id.
- Cargo features were renamed from `tinywasm-parser` to `parser` and from `logging` to `log`.
- `Error::ParseError` was renamed to `Error::Parser`, and `Error::Twasm` was added.
- `FuncHandle` and `FuncHandleTyped` were renamed to `Function` and `FunctionTyped`, and module export lookups moved from `exported_*` to `func_untyped`, `func`, and `memory`.
- The `twasm` archive format is now postcard-based and backwards-incompatible with previous versions (thanks [@dragonnn](https://github.com/dragonnn)).
- The interpreter was refactored around more superinstruction fusion, lower-overhead dispatch, typed-stack locals, jump-oriented lowering, and optional parallel parsing.

### Removed

- Cargo feature `simd` was removed.
- `RefNull` was removed and replaced with `FuncRef` and `ExternRef`.
- `tinywasm::interpreter` is no longer a public module.
- `InterpreterRuntime` and `TinyWasmValue` are no longer public API.
- `FuncHandle::name` was removed.
- Mutable `ModuleInstance` export lookup variants `memory_mut`, `table_mut`, `global_mut`, and `extern_item_mut` were removed.

### Fixed

- Fixed archive **no_std** support, which was broken in the previous release, and added tests to ensure it stays working.
- `ModuleInstance::memory` and `FuncContext::memory` are now actually immutable ([#41](https://github.com/explodingcamera/tinywasm/pull/41)).
- Untyped host functions now check return values correctly ([#27](https://github.com/explodingcamera/tinywasm/pull/27)) by [@WhaleKit](https://github.com/WhaleKit).
- `MemoryRefMut::copy_within(src, dst, len)` now follows its documented argument order.
- Imported tables created with `Extern::table(ty, init)` now honor the provided init value.
- Fixed unchecked memory offsets causing issues on 32-bit platforms.

### Migration Notes

- Replace `Store::new()` with `Store::default()` for default settings, or `Store::new(Engine::new(config))` for custom runtime configuration.
- Rename the cargo features `tinywasm-parser` to `parser` and `logging` to `log`.
- Rename `FuncHandle` to `Function` and `FuncHandleTyped` to `FunctionTyped`.
- Rename module export lookups from `exported_*` methods to `func`, `func_untyped`, and `memory`.
- Regenerate any persisted `twasm` archives; the format is now postcard-based and not backwards compatible with earlier releases.

## [0.8.0] - 2024-08-29

**All Commits**: https://github.com/explodingcamera/tinywasm/compare/v0.7.0...v0.8.0

### Added

- Full support for Multi-Memory proposal
- Improved support for WebAssembly 2.0 features

### Changed

- Extern tables now correctly update their type after growing
- Increased MSRV to 1.80.0
- Simplify and optimize the interpreter loop
- Use a separate stack and locals for 32, 64 and 128 bit values and references (#21)
- Updated to latest `wasmparser` version
- Removed benchmarks comparing TinyWasm to other WebAssembly runtimes to reduce build dependencies
- Memory and Data Instances are no longer reference counted

## [0.7.0] - 2024-05-15

**All Commits**: https://github.com/explodingcamera/tinywasm/compare/v0.6.0...v0.7.0

### Changed

- Remove all unsafe code
- Refactor interpreter loop
- Optimize Call-frames
- Remove unnecessary reference counter data from store

## [0.6.1] - 2024-05-10

**All Commits**: https://github.com/explodingcamera/tinywasm/compare/v0.6.0...v0.6.1

### Changed

- Switched back to the original `wasmparser` crate, which recently added support for `no_std`
- Performance improvements
- Updated dependencies

## [0.6.0] - 2024-03-27

**All Commits**: https://github.com/explodingcamera/tinywasm/compare/v0.5.0...v0.6.0

### Added

- `Imports` and `Module` are now cloneable (#9)

### Changed

- Improved documentation and added more tests
- Tests can now be run on more targets (#11)
- Nightly version has been updated to fix broken builds in some cases (#12)
- Add `aarch64-apple-darwin` and `armv7-unknown-linux-gnueabihf` targets to CI (#12)

### Removed

- Removed the `EndFunc` instruction, as it was already covered by the `Return` instruction\
  This also fixes a weird bug that only occurred on certain nightly versions of Rust

## [0.5.0] - 2024-03-01

**All Commits**: https://github.com/explodingcamera/tinywasm/compare/v0.4.0...v0.5.0

### Added

- Added this `CHANGELOG.md` file to the project
- Added merged instructions for improved performance and reduced bytecode size

### Changed

- Now using a custom `wasmparser` fork
- Switched to a visitor pattern for parsing WebAssembly modules
- Reduced the overhead of control flow instructions
- Reduced the size of bytecode instructions
- Fixed issues on the latest nightly Rust compiler
- Simplified a lot of the internal macros

### Removed

- Removed duplicate internal code

## [0.4.0] - 2024-02-04

**All Commits**: https://github.com/explodingcamera/tinywasm/compare/v0.3.0...v0.4.0

### Added

- Added benchmarks for comparison with other WebAssembly runtimes
- Added support for pre-processing WebAssembly modules into tinywasm bytecode
- Improved examples and documentation
- Implemented the bulk memory operations proposal

### Changed

- Overall performance improvements

## [0.3.0] - 2024-01-26

**All Commits**: https://github.com/explodingcamera/tinywasm/compare/v0.2.0...v0.3.0

- Better trap handling
- Implement linker
- Element instantiation
- Table Operations
- FuncRefs
- Typesafe host functions
- Host function context
- Spec compliance improvements
- Wasm 2.0 testsuite
- Usage examples
- End-to-end tests
- Lots of bug fixes
- Full `no_std` support

## [0.2.0] - 2024-01-11

**All Commits**: https://github.com/explodingcamera/tinywasm/compare/v0.1.0...v0.2.0

- Support for `br_table`
- Memory trapping improvements
- Implicit function label scopes
- else Instructions
- All Memory instructions
- Imports
- Basic linking
- Globals
- Fix function addr resolution
- Reference Instructions
