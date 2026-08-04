// https://codeql.github.com/codeql-query-help/rust/rust-access-invalid-pointer/
// Dereferencing an invalid or dangling pointer may cause undefined behavior.
//@compile-flags: -Z inline-mir=false

unsafe fn use_after_drop_len(ptr: *mut String) -> usize {
    unsafe {
        std::ptr::drop_in_place(ptr);
        (*ptr).len() //~ ERROR: dereferencing a pointer after it has been invalidated
    }
}

unsafe fn use_after_drop_index(ptr: *mut Vec<i32>) -> i32 {
    unsafe {
        std::ptr::drop_in_place(ptr);
        (*ptr)[0] //~ ERROR: dereferencing a pointer after it has been invalidated
    }
}

fn main() {}
