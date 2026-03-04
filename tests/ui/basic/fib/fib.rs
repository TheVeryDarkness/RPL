//@check-pass

// #[rpl::dump_mir(dump_cfg)]
pub fn han(n: usize) -> usize {
    match n {
        0 => 0,
        1 => 1,
        _ => han(n - 1) * 2 + 1,
    }
}

fn main() {
    let n = 10;
    let r = han(n);
    println!("fib({n}) = {r}");
}
