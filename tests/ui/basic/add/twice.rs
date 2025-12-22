//@ rustc-env: RPL_PATS=tests/ui/basic/add/add.rpl
fn main() {
    let x = 1usize;
    let y = 1usize;
    let z = 1usize;
    let w = x + y + z;
    //~^ ERROR: added value here
    //~| ERROR: added value here
}
