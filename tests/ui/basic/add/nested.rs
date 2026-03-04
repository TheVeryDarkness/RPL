//@ rustc-env: RPL_PATS=tests/ui/basic/add/add.rpl

#[inline(never)]
fn add(a: usize, b: usize) -> usize {
    a + b
    //~^ ERROR: added value here
}
#[inline(never)]
fn f1(x: usize, y: usize, z: usize) -> usize {
    add(add(x, y), z)
    //~^ ERROR: added value here
}
#[inline(never)]
fn f2(x: usize, y: usize, z: usize) -> usize {
    let w = add(x, y);
    //~^ ERROR: added value here
    add(w, z)
}
fn main() {}
