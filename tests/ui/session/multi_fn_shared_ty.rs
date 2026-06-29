//@rustc-env: RPL_PATS=tests/ui/session/multi_fn_shared_ty.rpl
//@ check-pass

#![allow(dead_code)]

struct Pair<T, U> {
    first: T,
    second: U,
}

fn uses_pair_a() {
    let _p = Pair { first: 1u8, second: 2u16 };
}

fn uses_pair_b() {
    let _p = Pair { first: 3u8, second: 4u16 };
}

fn main() {}
