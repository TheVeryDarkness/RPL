#![allow(rustc::usage_of_ty_tykind)]

// `'tcx` instead of `'pcx` used here because of the `rustc_arena::declare_arena` macro
#[macro_export]
macro_rules! arena_types {
    ($macro:path) => (
        $macro!([
            [] rpl_patterns: $crate::pat::RPLPattern<'tcx>,
            [] fn_patterns: $crate::pat::FnPattern<'tcx>,
            [] fn_pattern_bodys: $crate::pat::FnPatternBody<'tcx>,
        ]);
    )
}

arena_types!(rustc_arena::declare_arena);
