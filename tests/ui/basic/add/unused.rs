//@ rustc-env: RPL_PATS=tests/ui/basic/add/add.rpl

#[inline(never)]
fn add(a: usize, b: usize) -> usize {
    a + b
    //~^ ERROR: added value here
}
fn main() {}
