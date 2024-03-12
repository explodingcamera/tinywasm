# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

- Improved documentation and added more tests
- Tests can now be run on more targets
- Nightly version has been updated to fix broken builds in some cases
- Enhance support for scripted language bindings by making Imports and Module cloneable

### Removed

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
