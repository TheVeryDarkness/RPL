#![deny(clippy::correctness)]

pub mod cross_function {
    fn size_of<T>() -> usize {
        std::mem::size_of::<T>()
    }
    fn size_of_in_element_count<T: Copy, const N: usize>(p: *mut [T; N], q: *const [T; N]) {
        unsafe {
            p.copy_from(q, N * size_of::<[T; N]>()); //~ size_of_in_element_count
        }
    }

    fn null_ptr<T>() -> *const T {
        std::ptr::null()
    }
    fn transmute_null_to_fn<T>() -> fn() -> T {
        unsafe { std::mem::transmute(null_ptr::<T>()) } //~ transmute_null_to_fn
    }

    pub fn run() {
        let mut a = [1, 2, 3];
        let b = [4, 5, 6];
        size_of_in_element_count(&mut a, &b); //~ size_of_in_element_count

        let x = transmute_null_to_fn::<i32>()(); //~ transmute_null_to_fn
        dbg!(x);
    }
}

pub mod cross_statement {
    fn size_of_in_element_count<T: Copy, const N: usize>(p: *mut [T; N], q: *const [T; N]) {
        let count = N * std::mem::size_of::<[T; N]>(); //~ size_of_in_element_count
        unsafe {
            p.copy_from(q, count);
        }
    }

    pub fn run() {
        let mut a = [1, 2, 3];
        let b = [4, 5, 6];
        size_of_in_element_count(&mut a, &b); //~ size_of_in_element_count
    }
}

fn main() {
    cross_function::run();
    cross_statement::run();
}
