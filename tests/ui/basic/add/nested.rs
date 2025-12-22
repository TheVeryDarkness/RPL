//@ rustc-env: RPL_PATS=tests/ui/basic/add/add.rpl

#[inline(never)]
fn add(a: usize, b: usize) -> usize {
    a + b
    //~^ ERROR: added value here
}
fn main() {
    let x = 1usize;
    let y = 1usize;
    let z = 1usize;
    let w = add(add(x, y), z);
    //~^ ERROR: added value here
    //~| ERROR: added value here
}
