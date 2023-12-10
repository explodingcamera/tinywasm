//! This crate provides a way to access the WebAssembly spec testsuite.
//!
//! The testsuite is included as a git submodule and embedded into the binary.
//!
//! Generated from https://github.com/WebAssembly/testsuite

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

/// List of all proposals.
/// This list is generated from the `proposals` folder in https://github.com/WebAssembly/testsuite
#[rustfmt::skip] 
pub const PROPOSALS: &[&str] = &["annotations", "exception-handling", "memory64", "function-references", "multi-memory", "relaxed-simd", "tail-call", "threads", "extended-const", "gc"];

/// Get all test file names and their contents.
///
/// Proposals can be filtered by passing a list of proposal names.
/// Valid proposal names are listed in [`PROPOSALS`].
/// Returns an iterator over tuples of the form `(test_name, test_data)`.
/// test_name is the name of the test file and the proposal name (if any), e.g. `annotations/br.wast`.
pub fn get_tests(include_proposals: &[String]) -> impl Iterator<Item = (String, Cow<'static, [u8]>)> {
    let include_proposals = include_proposals.to_vec();

    Asset::iter().filter_map(move |x| {
        let mut parts = x.split('/');
        match parts.next() {
            Some("proposals") => {
                let proposal = parts.next();
                let test_name = parts.next().unwrap_or_default();

                if proposal.map_or(false, |p| include_proposals.contains(&p.to_string())) {
                    let full_path = format!("{}/{}", proposal.unwrap_or_default(), test_name);
                    let data = Asset::get(&x).unwrap().data;
                    Some((full_path, data))
                } else {
                    None
                }
            }
            Some(test_name) => {
                let data = Asset::get(&x).unwrap().data;
                Some((test_name.to_owned(), data))
            }
            None => None,
        }
    })
}

/// Get the WAST file as a byte slice.
///
/// # Examples
/// proposals: {proposal}/{test_name}.wast
///     tests: {test_name}.wast
pub fn get_wast(name: &str) -> Option<Cow<'_, [u8]>> {
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
            // assert!(PROPOSALS.contains(&proposal));
        }
        println!("{:?}", unique_proposals);
    }

    #[test]
    fn test_get_tests() {
        let tests = get_tests(&["annotations".to_owned()]);
        let tests: Vec<_> = tests.collect();
        println!("{:?}", tests.iter().map(|(name, _)| name).collect::<Vec<_>>());

        // for (name, data) in tests {
        //     println!("{}: {}", name, data.len());
        // }
    }
}
