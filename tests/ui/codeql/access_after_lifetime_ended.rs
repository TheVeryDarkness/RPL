// https://codeql.github.com/codeql-query-help/rust/rust-access-after-lifetime-ended/
// Dereferencing a pointer after the lifetime of its target has ended
// causes undefined behavior.

fn get_const_pointer() -> *const i64 {
    let val = 123;
    &raw const val //~ ERROR: returning a pointer to a local variable whose lifetime is about to end
}

fn get_mut_pointer() -> *mut i32 {
    let mut val = 456;
    &raw mut val //~ ERROR: returning a pointer to a local variable whose lifetime is about to end
}

fn get_pointer_to_struct() -> *const String {
    let val = String::from("hello");
    &raw const val //~ ERROR: returning a pointer to a local variable whose lifetime is about to end
}

fn get_pointer_u8() -> *const u8 {
    let val: u8 = 42;
    &raw const val //~ ERROR: returning a pointer to a local variable whose lifetime is about to end
}

fn main() {}
