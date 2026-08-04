// https://codeql.github.com/codeql-query-help/rust/rust-ctor-initialization/
// Calling std library functions from a #[ctor] or #[dtor] function is not safe,
// because std only guarantees stability between the beginning and end of main.
//@compile-flags: -Z inline-mir=false

#[ctor::ctor]
fn bad_ctor_println() {
    println!("Hello from ctor!"); //~ ERROR: calling `std::io::_print` in a `#[ctor]`/`#[dtor]` context is unsafe
}

#[ctor::ctor]
fn bad_ctor_print() {
    print!("Hello from ctor!"); //~ ERROR: calling `std::io::_print` in a `#[ctor]`/`#[dtor]` context is unsafe
}

#[ctor::ctor]
fn bad_ctor_stdout() {
    use std::io::Write;
    let mut handle = std::io::stdout(); //~ ERROR: calling `std::io::stdout` in a `#[ctor]`/`#[dtor]` context is unsafe
    handle.write_all(b"Hello from ctor!\n").unwrap();
}

fn main() {}
