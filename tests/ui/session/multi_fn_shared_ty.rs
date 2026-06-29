//@rustc-env: RPL_PATS=tests/ui/session/multi_fn_shared_ty.rpl

#![allow(dead_code)]

struct Pair<T, U> {
    first: T,
    second: U,
}

fn uses_pair_a(_: ()) { //~ ERROR: multi-function pattern with shared type variables matched
    let _p = Pair { first: 1u8, second: 2u16 };
}

fn uses_pair_b(_: ()) { //~ ERROR: multi-function pattern with shared type variables matched
    let _p = Pair { first: 3u8, second: 4u16 };
}

fn main() {}
