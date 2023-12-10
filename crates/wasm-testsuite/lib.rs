//! This crate provides a way to access the WebAssembly spec testsuite.
//!
//! The testsuite is included as a git submodule and embedded into the binary.
//!
//! Generated from <https://github.com/WebAssembly/testsuite>

#![forbid(unsafe_code)]
#![doc(test(
    no_crate_inject,
    attr(
        deny(warnings, rust_2018_idioms),
        allow(dead_code, unused_assignments, unused_variables)
    )
))]
#![warn(missing_docs, missing_debug_implementations, rust_2018_idioms, unreachable_pub)]

use rust_embed::RustEmbed;
use std::borrow::Cow;

#[derive(RustEmbed)]
#[folder = "data/"]
#[include = "*.wast"]
struct Asset;

/// List of all supported proposals. Can be used to filter tests.
/// 
/// Includes all proposals from <https://github.com/WebAssembly/testsuite/tree/master/proposals>
#[rustfmt::skip] 
pub const PROPOSALS: &[&str] = &["annotations", "exception-handling", "memory64", "function-references", "multi-memory", "relaxed-simd", "tail-call", "threads", "extended-const", "gc"];

/// List of all tests that apply to the MVP (V1) spec.
/// Note that the tests are still for the latest spec, so the latest version of Wast is used.
#[rustfmt::skip] 
pub const MVP_TESTS: &[&str] = &["address.wast","address.wast","align.wast","align.wast","binary-leb128.wast","binary-leb128.wast","binary.wast","binary.wast","block.wast","block.wast","br.wast","br.wast","br_if.wast","br_if.wast","br_table.wast","br_table.wast","break-drop.wast","break-drop.wast","call.wast","call.wast","call_indirect.wast","call_indirect.wast","comments.wast","comments.wast","const.wast","const.wast","conversions.wast","conversions.wast","custom.wast","custom.wast","data.wast","data.wast","elem.wast","elem.wast","endianness.wast","endianness.wast","exports.wast","exports.wast","f32.wast","f32.wast","f32_bitwise.wast","f32_bitwise.wast","f32_cmp.wast","f32_cmp.wast","f64.wast","f64.wast","f64_bitwise.wast","f64_bitwise.wast","f64_cmp.wast","f64_cmp.wast","fac.wast","fac.wast","float_exprs.wast","float_exprs.wast","float_literals.wast","float_literals.wast","float_memory.wast","float_memory.wast","float_misc.wast","float_misc.wast","forward.wast","forward.wast","func.wast","func.wast","func_ptrs.wast","func_ptrs.wast","globals.wast","globals.wast","i32.wast","i32.wast","i64.wast","i64.wast","if.wast","if.wast","imports.wast","imports.wast","inline-module.wast","inline-module.wast","int_exprs.wast","int_exprs.wast","int_literals.wast","int_literals.wast","labels.wast","labels.wast","left-to-right.wast","left-to-right.wast","linking.wast","linking.wast","load.wast","load.wast","local_get.wast","local_get.wast","local_set.wast","local_set.wast","local_tee.wast","local_tee.wast","loop.wast","loop.wast","memory.wast","memory.wast","memory_grow.wast","memory_grow.wast","memory_redundancy.wast","memory_redundancy.wast","memory_size.wast","memory_size.wast","memory_trap.wast","memory_trap.wast","names.wast","names.wast","nop.wast","nop.wast","return.wast","return.wast","select.wast","select.wast","skip-stack-guard-page.wast","skip-stack-guard-page.wast","stack.wast","stack.wast","start.wast","start.wast","store.wast","store.wast","switch.wast","switch.wast","token.wast","token.wast","traps.wast","traps.wast","type.wast","type.wast","unreachable.wast","unreachable.wast","unreached-invalid.wast","unreached-invalid.wast","unwind.wast","unwind.wast","utf8-custom-section-id.wast","utf8-custom-section-id.wast","utf8-import-field.wast","utf8-import-field.wast","utf8-import-module.wast","utf8-import-module.wast","utf8-invalid-encoding.wast","utf8-invalid-encoding.wast"];

/// List of all tests that apply to the V2 draft 1 spec.
#[rustfmt::skip]
pub const V2_DRAFT_1_TESTS: &[&str] = &["address.wast","address.wast","align.wast","align.wast","binary-leb128.wast","binary-leb128.wast","binary.wast","binary.wast","block.wast","block.wast","br.wast","br.wast","br_if.wast","br_if.wast","br_table.wast","br_table.wast","bulk.wast","bulk.wast","call.wast","call.wast","call_indirect.wast","call_indirect.wast","comments.wast","comments.wast","const.wast","const.wast","conversions.wast","conversions.wast","custom.wast","custom.wast","data.wast","data.wast","elem.wast","elem.wast","endianness.wast","endianness.wast","exports.wast","exports.wast","f32.wast","f32.wast","f32_bitwise.wast","f32_bitwise.wast","f32_cmp.wast","f32_cmp.wast","f64.wast","f64.wast","f64_bitwise.wast","f64_bitwise.wast","f64_cmp.wast","f64_cmp.wast","fac.wast","fac.wast","float_exprs.wast","float_exprs.wast","float_literals.wast","float_literals.wast","float_memory.wast","float_memory.wast","float_misc.wast","float_misc.wast","forward.wast","forward.wast","func.wast","func.wast","func_ptrs.wast","func_ptrs.wast","global.wast","global.wast","i32.wast","i32.wast","i64.wast","i64.wast","if.wast","if.wast","imports.wast","imports.wast","inline-module.wast","inline-module.wast","int_exprs.wast","int_exprs.wast","int_literals.wast","int_literals.wast","labels.wast","labels.wast","left-to-right.wast","left-to-right.wast","linking.wast","linking.wast","load.wast","load.wast","local_get.wast","local_get.wast","local_set.wast","local_set.wast","local_tee.wast","local_tee.wast","loop.wast","loop.wast","memory.wast","memory.wast","memory_copy.wast","memory_copy.wast","memory_fill.wast","memory_fill.wast","memory_grow.wast","memory_grow.wast","memory_init.wast","memory_init.wast","memory_redundancy.wast","memory_redundancy.wast","memory_size.wast","memory_size.wast","memory_trap.wast","memory_trap.wast","names.wast","names.wast","nop.wast","nop.wast","ref_func.wast","ref_func.wast","ref_is_null.wast","ref_is_null.wast","ref_null.wast","ref_null.wast","return.wast","return.wast","select.wast","select.wast","skip-stack-guard-page.wast","skip-stack-guard-page.wast","stack.wast","stack.wast","start.wast","start.wast","store.wast","store.wast","switch.wast","switch.wast","table-sub.wast","table-sub.wast","table.wast","table.wast","table_copy.wast","table_copy.wast","table_fill.wast","table_fill.wast","table_get.wast","table_get.wast","table_grow.wast","table_grow.wast","table_init.wast","table_init.wast","table_set.wast","table_set.wast","table_size.wast","table_size.wast","token.wast","token.wast","traps.wast","traps.wast","type.wast","type.wast","unreachable.wast","unreachable.wast","unreached-invalid.wast","unreached-invalid.wast","unreached-valid.wast","unreached-valid.wast","unwind.wast","unwind.wast","utf8-custom-section-id.wast","utf8-custom-section-id.wast","utf8-import-field.wast","utf8-import-field.wast","utf8-import-module.wast","utf8-import-module.wast","utf8-invalid-encoding.wast","utf8-invalid-encoding.wast"];

/// Get all test file names and their contents.
pub fn get_tests_wast(include_proposals: &[String]) -> impl Iterator<Item = (String, Cow<'static, [u8]>)> {
    get_tests(&include_proposals)
        .filter_map(|name| Some((name.clone(), get_test_wast(&name)?)))
        .map(|(name, data)| (name, Cow::Owned(data.to_vec())))
}

/// Get all test file names.
pub fn get_tests(include_proposals: &[String]) -> impl Iterator<Item = String> {
    let include_proposals = include_proposals.to_vec();

    Asset::iter().filter_map(move |x| {
        let mut parts = x.split('/');
        match parts.next() {
            Some("proposals") => {
                let proposal = parts.next();
                let test_name = parts.next().unwrap_or_default();

                if proposal.map_or(false, |p| include_proposals.contains(&p.to_string())) {
                    let full_path = format!("{}/{}", proposal.unwrap_or_default(), test_name);
                    Some(full_path)
                } else {
                    None
                }
            }
            Some(test_name) => Some(test_name.to_owned()),
            None => None,
        }
    })
}

/// Get the WAST file as a byte slice.
pub fn get_test_wast(name: &str) -> Option<Cow<'static, [u8]>> {
    if !name.ends_with(".wast") {
        panic!("Expected .wast file. Got: {}", name);
    }

    match name.contains('/') {
        true => Asset::get(&format!("proposals/{}", name)).map(|x| x.data),
        false => Asset::get(name).map(|x| x.data),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn test_proposals() {
        let mut unique_proposals = HashSet::new();

        // check that all proposals are present
        for proposal in Asset::iter() {
            if !proposal.starts_with("proposals/") {
                continue;
            }

            let proposal = proposal.split('/').nth(1).unwrap();
            unique_proposals.insert(proposal.to_owned());
            assert!(PROPOSALS.contains(&proposal));
        }
    }
}
