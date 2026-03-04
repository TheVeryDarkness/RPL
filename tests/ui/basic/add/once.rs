//@ rustc-env: RPL_PATS=tests/ui/basic/add/add.rpl
fn main() {
    let x = 1usize;
    let y = 1usize;
    let z = x + y;
    //~^ ERROR: added value here
}
