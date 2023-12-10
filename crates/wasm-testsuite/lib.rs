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

/// List of all proposals. Used to filter tests.
/// 
/// Includes all proposals from <https://github.com/WebAssembly/testsuite/tree/master/proposals>
#[rustfmt::skip] 
pub const PROPOSALS: &[&str] = &["annotations", "exception-handling", "memory64", "function-references", "multi-memory", "relaxed-simd", "tail-call", "threads", "extended-const", "gc"];

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
