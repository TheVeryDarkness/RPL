//@check-pass
use std::cell::UnsafeCell;
use std::rc::Rc;

pub struct Cell<T> {
    pub inner: Rc<std::cell::RefCell<T>>,
}

impl<T> Clone for Cell<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Cell<T> {
    pub fn get_mut(&mut self) -> impl std::ops::DerefMut<Target = T> + '_ {
        self.inner.as_ref().borrow_mut()
    }
}
