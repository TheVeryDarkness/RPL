//@ignore-on-host: check individual modules instead
#![deny(clippy::correctness)]
#![allow(internal_features)]
#![feature(rustc_attrs)]
mod eager_transmute;
mod mem_replace_with_uninit;
mod mut_from_ref;
mod not_unsafe_ptr_arg_deref;
mod size_of_in_element_count;
mod transmute_collection;
mod transmute_null_to_fn;
mod transmuting_null;
mod uninit_assumed_init;
mod uninit_vec;
mod zero_offset;

fn main() {
    eager_transmute::main();
    mem_replace_with_uninit::main();
    mut_from_ref::main();
    not_unsafe_ptr_arg_deref::main();
    size_of_in_element_count::main();
    transmute_collection::main();
    transmuting_null::main();
    transmute_null_to_fn::main();
    uninit_assumed_init::main();
    uninit_vec::main();
    zero_offset::main();
}
