//@revisions: inline normal
//@[inline] compile-flags: -Zinline-mir=false
#[cfg_attr(test, test)]
fn base_case() {
    unsafe {
        let x: &u64 = std::mem::transmute(0 as *const u64);
        //~^ transmuting_null
        dbg!(x);

        let x: &u64 = std::mem::transmute(std::ptr::null::<u64>());
        //~^ transmuting_null
        dbg!(x);
    }
}

#[cfg_attr(test, test)]
fn cross_function_null_ptr() {
    const fn const_null_ptr<T>() -> *const T {
        std::ptr::null()
    }
    fn null_ptr<T>() -> *const T {
        std::ptr::null()
    }
    unsafe {
        let x: &u64 = std::mem::transmute(const_null_ptr::<u64>());
        //~^ transmuting_null
        dbg!(x);

        let x: &u64 = std::mem::transmute(null_ptr::<u64>());
        //~^ transmuting_null
        dbg!(x);
    }
}

#[cfg_attr(test, test)]
fn cross_statement() {
    let null_ptr = 0 as *const u64;
    unsafe {
        let x: &u64 = std::mem::transmute(null_ptr);
        //~^ transmuting_null
        dbg!(x);
    }

    let null_ptr = std::ptr::null::<u64>();
    unsafe {
        let x: &u64 = std::mem::transmute(null_ptr);
        //~^ transmuting_null
        dbg!(x);
    }
}

pub(crate) fn main() {
    base_case();
    cross_function_null_ptr();
    cross_statement();
}
