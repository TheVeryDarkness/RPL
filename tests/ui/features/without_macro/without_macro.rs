//@rustc-env: RPL_PATS=tests/ui/features/without_macro/without_macro.rpl
//@check-pass
//@compile-flags: -Z inline-mir=false

//! `without!` macro sugar: desugars to a pattern difference `{ ... } - { ... }`.
//! The desugaring runs when the pattern is loaded; this test verifies the
//! pattern loads and the driver runs without crashing. The difference semantics
//! are covered by `session/op_subtract` and the `rpl_context` macro tests.

#![allow(dead_code)]

fn main() {}
