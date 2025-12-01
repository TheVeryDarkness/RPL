use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::Ordering;

/// An Atom wraps an AtomicPtr, it allows for safe mutation of an atomic
/// into common Rust Types.
pub struct Atom<P>
where
    P: IntoRawPtr + FromRawPtr,
{
    inner: AtomicPtr<()>,
    data: PhantomData<UnsafeCell<P>>,
}

unsafe impl<P> Send for Atom<P> where P: IntoRawPtr + FromRawPtr {}
unsafe impl<P> Sync for Atom<P> where P: IntoRawPtr + FromRawPtr {}

/// Convert from into a raw pointer
pub trait IntoRawPtr {
    unsafe fn into_raw(self) -> *mut ();
}

/// Convert from a raw ptr into a pointer
pub trait FromRawPtr {
    unsafe fn from_raw(ptr: *mut ()) -> Self;
}

impl<T> IntoRawPtr for Box<T> {
    #[inline]
    unsafe fn into_raw(self) -> *mut () {
        Box::into_raw(self) as *mut ()
    }
}

impl<T> FromRawPtr for Box<T> {
    #[inline]
    unsafe fn from_raw(ptr: *mut ()) -> Box<T> {
        Box::from_raw(ptr as *mut T)
    }
}

impl<T> IntoRawPtr for Arc<T> {
    #[inline]
    unsafe fn into_raw(self) -> *mut () {
        Arc::into_raw(self) as *mut T as *mut ()
    }
}

impl<T> FromRawPtr for Arc<T> {
    #[inline]
    unsafe fn from_raw(ptr: *mut ()) -> Arc<T> {
        Arc::from_raw(ptr as *const () as *const T)
    }
}

// This impl can be useful for stack-allocated and 'static values.
impl<'a, T> IntoRawPtr for &'a T {
    #[inline]
    unsafe fn into_raw(self) -> *mut () {
        self as *const _ as *mut ()
    }
}

impl<'a, T> FromRawPtr for &'a T {
    #[inline]
    unsafe fn from_raw(ptr: *mut ()) -> &'a T {
        &*(ptr as *mut T)
    }
}

fn main() {}
