# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [UNRELEASED]

### Changed

- Improved support for WebAssembly 2.0 features
- Simplify and optimize the interpreter loop
- Use a seperate stack and locals for 32, 64 and 128 bit values and references (#21)
- Updated to latest wasmparser version

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

## [0.3.0] - 2024-01-11

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
