#![doc = include_str!("README.md")]
#![forbid(unsafe_code)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_assignments, unused_variables))
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
pub const PROPOSALS: &[&str] = &["annotations", "exception-handling", "extended-const", "function-references", "gc", "memory64", "multi-memory", "relaxed-simd", "tail-call", "threads"];

/// List of all tests that apply to the MVP (V1) spec
/// Note that the tests are still for the latest spec, so the latest version of Wast is used.
#[rustfmt::skip]  // removed: "break-drop.wast",
pub const MVP_TESTS: &[&str] = &["address.wast", "align.wast", "binary-leb128.wast", "binary.wast", "block.wast", "br.wast", "br_if.wast", "br_table.wast", "call.wast", "call_indirect.wast", "comments.wast", "const.wast", "conversions.wast", "custom.wast", "data.wast", "elem.wast", "endianness.wast", "exports.wast", "f32.wast", "f32_bitwise.wast", "f32_cmp.wast", "f64.wast", "f64_bitwise.wast", "f64_cmp.wast", "fac.wast", "float_exprs.wast", "float_literals.wast", "float_memory.wast", "float_misc.wast", "forward.wast", "func.wast", "func_ptrs.wast", "global.wast", "i32.wast", "i64.wast", "if.wast", "imports.wast", "inline-module.wast", "int_exprs.wast", "int_literals.wast", "labels.wast", "left-to-right.wast", "linking.wast", "load.wast", "local_get.wast", "local_set.wast", "local_tee.wast", "loop.wast", "memory.wast", "memory_grow.wast", "memory_redundancy.wast", "memory_size.wast", "memory_trap.wast", "names.wast", "nop.wast", "return.wast", "select.wast", "skip-stack-guard-page.wast", "stack.wast", "start.wast", "store.wast", "switch.wast", "table.wast", "token.wast", "traps.wast", "type.wast", "unreachable.wast", "unreached-valid.wast", "unreached-invalid.wast", "unwind.wast", "utf8-custom-section-id.wast", "utf8-import-field.wast", "utf8-import-module.wast", "utf8-invalid-encoding.wast"];

/// List of all tests that apply to the V2 draft 1 spec.
#[rustfmt::skip]
pub const V2_DRAFT_1_TESTS: &[&str] = &["address.wast", "align.wast", "binary-leb128.wast", "binary.wast", "block.wast", "br.wast", "br_if.wast", "br_table.wast", "bulk.wast", "call.wast", "call_indirect.wast", "comments.wast", "const.wast", "conversions.wast", "custom.wast", "data.wast", "elem.wast", "endianness.wast", "exports.wast", "f32.wast", "f32_bitwise.wast", "f32_cmp.wast", "f64.wast", "f64_bitwise.wast", "f64_cmp.wast", "fac.wast", "float_exprs.wast", "float_literals.wast", "float_memory.wast", "float_misc.wast", "forward.wast", "func.wast", "func_ptrs.wast", "global.wast", "i32.wast", "i64.wast", "if.wast", "imports.wast", "inline-module.wast", "int_exprs.wast", "int_literals.wast", "labels.wast", "left-to-right.wast", "linking.wast", "load.wast", "local_get.wast", "local_set.wast", "local_tee.wast", "loop.wast", "memory.wast", "memory_copy.wast", "memory_fill.wast", "memory_grow.wast", "memory_init.wast", "memory_redundancy.wast", "memory_size.wast", "memory_trap.wast", "names.wast", "nop.wast", "obsolete-keywords.wast", "ref_func.wast", "ref_is_null.wast", "ref_null.wast", "return.wast", "select.wast", "skip-stack-guard-page.wast", "stack.wast", "start.wast", "store.wast", "switch.wast", "table-sub.wast", "table.wast", "table_copy.wast", "table_fill.wast", "table_get.wast", "table_grow.wast", "table_init.wast", "table_set.wast", "table_size.wast", "token.wast", "traps.wast", "type.wast", "unreachable.wast", "unreached-invalid.wast", "unreached-valid.wast", "unwind.wast", "utf8-custom-section-id.wast", "utf8-import-field.wast", "utf8-import-module.wast", "utf8-invalid-encoding.wast"];

/// List of all tests that apply to the simd proposal
#[rustfmt::skip]
pub const SIMD_TESTS: &[&str] = &["simd_address.wast", "simd_align.wast", "simd_bit_shift.wast", "simd_bitwise.wast", "simd_boolean.wast", "simd_const.wast", "simd_conversions.wast", "simd_f32x4.wast", "simd_f32x4_arith.wast", "simd_f32x4_cmp.wast", "simd_f32x4_pmin_pmax.wast", "simd_f32x4_rounding.wast", "simd_f64x2.wast", "simd_f64x2_arith.wast", "simd_f64x2_cmp.wast", "simd_f64x2_pmin_pmax.wast", "simd_f64x2_rounding.wast", "simd_i16x8_arith.wast", "simd_i16x8_arith2.wast", "simd_i16x8_cmp.wast", "simd_i16x8_extadd_pairwise_i8x16.wast", "simd_i16x8_extmul_i8x16.wast", "simd_i16x8_q15mulr_sat_s.wast", "simd_i16x8_sat_arith.wast", "simd_i32x4_arith.wast", "simd_i32x4_arith2.wast", "simd_i32x4_cmp.wast", "simd_i32x4_dot_i16x8.wast", "simd_i32x4_extadd_pairwise_i16x8.wast", "simd_i32x4_extmul_i16x8.wast", "simd_i32x4_trunc_sat_f32x4.wast", "simd_i32x4_trunc_sat_f64x2.wast", "simd_i64x2_arith.wast", "simd_i64x2_arith2.wast", "simd_i64x2_cmp.wast", "simd_i64x2_extmul_i32x4.wast", "simd_i8x16_arith.wast", "simd_i8x16_arith2.wast", "simd_i8x16_cmp.wast", "simd_i8x16_sat_arith.wast", "simd_int_to_int_extend.wast", "simd_lane.wast", "simd_linking.wast", "simd_load.wast", "simd_load16_lane.wast", "simd_load32_lane.wast", "simd_load64_lane.wast", "simd_load8_lane.wast", "simd_load_extend.wast", "simd_load_splat.wast", "simd_load_zero.wast", "simd_splat.wast", "simd_store.wast", "simd_store16_lane.wast", "simd_store32_lane.wast", "simd_store64_lane.wast", "simd_store8_lane.wast"];

/// List of all tests that apply to a specific proposal.
pub fn get_proposal_tests(proposal: &str) -> impl Iterator<Item = String> + '_ {
    Asset::iter().filter_map(move |x| {
        let mut parts = x.split('/');
        if parts.next() == Some("proposals") && parts.next() == Some(proposal) {
            Some(format!("{}/{}", proposal, parts.next().unwrap_or_default()))
        } else {
            None
        }
    })
}

/// Get the WAST file as a byte slice.
pub fn get_test_wast(name: &str) -> Option<Cow<'static, [u8]>> {
    assert!(name.ends_with(".wast"), "Expected .wast file. Got: {name}");

    match name.contains('/') {
        true => Asset::get(&format!("proposals/{name}")).map(|x| x.data),
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
