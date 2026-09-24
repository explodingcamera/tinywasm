# tinywasm-component-abi

This optional `no_std + alloc` crate starts with checked, borrowed wasm32
canonical-ABI access for UTF-8 strings, `list<u8>`, and `list<u32>`. Hosts
provide the memory handle and a per-transfer byte limit; reads do not copy or
allocate. The crate has no dependency on `tinywasm-wasi`, and neither the core
interpreter nor the WASI Preview 1 calling path depends on it.

This is **not** a component runtime. It does not parse or instantiate
components, implement WASI Preview 2/3 modules, call `realloc`, manage resource
handles, or bridge async calls. It is a small foundation for custom WIT host
modules; full components and cross-language async can be built incrementally
without making Preview 1 users pay for them.

Hosts still need store fuel/time limits, memory/resource quotas, and their own
total-work budgets. The byte limit here applies to each transfer only.
