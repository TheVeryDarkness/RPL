//@ignore-target: windows

// See https://github.com/gnzlbg/slice_deque/blob/621274a01226a8a700f6bf9c8cf5a9909567867b
#![allow(deprecated)]
#![allow(invalid_value)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(rpl::generic_function_marked_inline)]
#![allow(rpl::private_function_marked_inline)]
#![allow(rpl::ptr_offset_with_cast)]
use core::ptr::NonNull;
use core::{cmp, convert, fmt, hash, iter, mem, ops, ptr, slice, str};
use mirrored::{allocate_mirrored, allocation_granularity, deallocate_mirrored};

/// Allocation error.
pub enum AllocError {
    /// The system is Out-of-memory.
    Oom,
    /// Other allocation errors (not out-of-memory).
    ///
    /// Race conditions, exhausted file descriptors, etc.
    Other,
}

impl crate::fmt::Debug for AllocError {
    fn fmt(&self, f: &mut crate::fmt::Formatter) -> crate::fmt::Result {
        match self {
            AllocError::Oom => write!(f, "out-of-memory"),
            AllocError::Other => write!(f, "other (not out-of-memory)"),
        }
    }
}

/// A stable version of the `core::intrinsics` module.
#[cfg(not(feature = "unstable"))]
mod intrinsics {
    /// Like `core::intrinsics::unlikely` but does nothing.
    #[inline(always)]
    pub unsafe fn unlikely<T>(x: T) -> T {
        x
    }

    /// Like `core::intrinsics::assume` but does nothing.
    #[inline(always)]
    pub unsafe fn assume<T>(x: T) -> T {
        x
    }

    /// Like `core::intrinsics::arith_offset` but doing pointer to integer
    /// conversions.
    #[inline(always)]
    pub unsafe fn arith_offset<T>(dst: *const T, offset: isize) -> *const T {
        let r = if offset >= 0 {
            (dst as usize).wrapping_add(offset as usize)
        } else {
            (dst as usize).wrapping_sub((-offset) as usize)
        };
        r as *const T
    }
}

/// A double-ended queue that derefs into a slice.
///
/// It is implemented with a growable virtual ring buffer.
pub struct SliceDeque<T> {
    /// Index of the first element in the queue.
    head_: usize,
    /// Index of one past the last element in the queue.
    tail_: usize,
    /// Mirrored memory buffer.
    buf: Buffer<T>,
}

impl<T> SliceDeque<T> {
    /// Creates a new empty deque.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use slice_deque::SliceDeque;
    /// let deq = SliceDeque::new();
    /// # let o: SliceDeque<u32> = deq;
    /// ```
    #[inline]
    pub fn new() -> Self {
        Self {
            head_: 0,
            tail_: 0,
            buf: Buffer::new(),
        }
    }

    /// Returns the number of elements that the deque can hold without
    /// reallocating.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use slice_deque::SliceDeque;
    /// let deq = SliceDeque::with_capacity(10);
    /// assert!(deq.capacity() >= 10);
    /// # let o: SliceDeque<u32> = deq;
    /// ```
    #[inline]
    pub fn capacity(&self) -> usize {
        // Note: the buffer length is not necessarily a power of two
        // debug_assert!(self.buf.len() % 2 == 0);
        self.buf.len() / 2
    }

    /// Largest tail value
    #[inline]
    fn tail_upper_bound(&self) -> usize {
        self.capacity() * 2
    }

    /// Largest head value
    #[inline]
    fn head_upper_bound(&self) -> usize {
        self.capacity()
    }

    /// Get index to the head
    #[inline]
    fn head(&self) -> usize {
        self.head_
    }

    /// Get index to the tail
    #[inline]
    fn tail(&self) -> usize {
        self.tail_
    }

    /// Provides a reference to the last element, or `None` if the deque is
    /// empty.
    ///
    /// # Examples
    ///
    /// ```
    /// # use slice_deque::SliceDeque;
    /// let mut deq = SliceDeque::new();
    /// assert_eq!(deq.back(), None);
    ///
    /// deq.push_back(1);
    /// deq.push_back(2);
    /// assert_eq!(deq.back(), Some(&2));
    /// deq.push_front(3);
    /// assert_eq!(deq.back(), Some(&2));
    /// ```
    #[inline]
    pub fn back(&self) -> Option<&T> {
        let last_idx = self.len().wrapping_sub(1);
        self.get(last_idx)
    }

    /// Number of elements in the ring buffer.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use slice_deque::SliceDeque;
    /// let mut deq = SliceDeque::with_capacity(10);
    /// assert!(deq.len() == 0);
    /// deq.push_back(3);
    /// assert!(deq.len() == 1);
    /// ```
    #[inline]
    pub fn len(&self) -> usize {
        let l = self.tail() - self.head();
        debug_assert!(self.tail() >= self.head());
        debug_assert!(l <= self.capacity());
        l
    }

    /// Is the ring buffer full ?
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use slice_deque::SliceDeque;
    /// let mut deq = SliceDeque::with_capacity(10);
    /// assert!(!deq.is_full());
    /// # let o: SliceDeque<u32> = deq;
    /// ```
    #[inline]
    pub fn is_full(&self) -> bool {
        self.len() == self.capacity()
    }

    /// Extracts a slice containing the entire deque.
    #[inline]
    pub fn as_slice(&self) -> &[T] {
        unsafe {
            let ptr = self.buf.ptr();
            let ptr = ptr.add(self.head());
            slice::from_raw_parts(ptr, self.len())
        }
    }

    /// Extracts a mutable slice containing the entire deque.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe {
            let ptr = self.buf.ptr();
            let ptr = ptr.add(self.head());
            slice::from_raw_parts_mut(ptr, self.len())
        }
    }
    /// Attempts to reserve capacity for inserting at least `additional`
    /// elements without reallocating. Does nothing if the capacity is already
    /// sufficient.
    ///
    /// The collection always reserves memory in multiples of the page size.
    ///
    /// # Panics
    ///
    /// Panics if the new capacity overflows `usize`.
    #[inline]
    pub fn try_reserve(&mut self, additional: usize) -> Result<(), AllocError> {
        let old_len = self.len();
        let new_cap = self.grow_policy(additional);
        self.reserve_capacity(new_cap)?;
        debug_assert!(self.capacity() >= old_len + additional);
        Ok(())
    }

    /// Attempts to reserve capacity for `new_capacity` elements. Does nothing
    /// if the capacity is already sufficient.
    #[inline]
    fn reserve_capacity(&mut self, new_capacity: usize) -> Result<(), AllocError> {
        unsafe {
            if new_capacity <= self.capacity() {
                return Ok(());
            }

            let mut new_buffer = Buffer::uninitialized(2 * new_capacity)?;
            debug_assert!(new_buffer.len() >= 2 * new_capacity);

            let len = self.len();
            // Move the elements from the current buffer
            // to the beginning of the new buffer:
            {
                let from_ptr = self.as_mut_ptr();
                let to_ptr = new_buffer.as_mut_slice().as_mut_ptr();
                crate::ptr::copy_nonoverlapping(from_ptr, to_ptr, len);
            }

            // Exchange buffers
            mem::swap(&mut self.buf, &mut new_buffer);

            // Correct head and tail (we copied to the
            // beginning of the of the new buffer)
            self.head_ = 0;
            self.tail_ = len;

            Ok(())
        }
    }

    /// Growth policy of the deque. The capacity is going to be a multiple of
    /// the page-size anyways, so we just double capacity when needed.
    #[inline]
    fn grow_policy(&self, additional: usize) -> usize {
        let cur_cap = self.capacity();
        let old_len = self.len();
        let req_cap = old_len.checked_add(additional).expect("overflow");
        if req_cap > cur_cap {
            let dbl_cap = cur_cap.saturating_mul(2);
            cmp::max(req_cap, dbl_cap)
        } else {
            req_cap
        }
    }

    /// Attempts to prepend `value` to the deque.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use slice_deque::SliceDeque;
    /// let mut deq = SliceDeque::new();
    /// deq.try_push_front(1).unwrap();
    /// deq.try_push_front(2).unwrap();
    /// assert_eq!(deq.front(), Some(&2));
    /// ```
    #[inline]
    pub fn try_push_front(&mut self, value: T) -> Result<(), (T, AllocError)> {
        unsafe {
            if intrinsics::unlikely(self.is_full()) {
                if let Err(e) = self.try_reserve(1) {
                    return Err((value, e));
                }
            }

            self.move_head_unchecked(-1);
            ptr::write(self.get_mut(0).unwrap(), value);
            Ok(())
        }
    }

    /// Prepends `value` to the deque.
    ///
    /// # Panics
    ///
    /// On OOM.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use slice_deque::SliceDeque;
    /// let mut deq = SliceDeque::new();
    /// deq.push_front(1);
    /// deq.push_front(2);
    /// assert_eq!(deq.front(), Some(&2));
    /// ```
    #[inline]
    pub fn push_front(&mut self, value: T) {
        if let Err(e) = self.try_push_front(value) {
            panic!("{:?}", e.1);
        }
    }

    /// Removes the first element and returns it, or `None` if the deque is
    /// empty.
    ///
    /// # Examples
    ///
    /// ```
    /// # use slice_deque::SliceDeque;
    /// let mut deq = SliceDeque::new();
    /// assert_eq!(deq.pop_front(), None);
    ///
    /// deq.push_back(1);
    /// deq.push_back(2);
    ///
    /// assert_eq!(deq.pop_front(), Some(1));
    /// assert_eq!(deq.pop_front(), Some(2));
    /// assert_eq!(deq.pop_front(), None);
    /// ```
    #[inline]
    pub fn pop_front(&mut self) -> Option<T> {
        unsafe {
            let v = match self.get_mut(0) {
                None => return None,
                Some(v) => ptr::read(v),
            };
            self.move_head_unchecked(1);
            Some(v)
        }
    }

    /// Moves the deque head by `x`.
    ///
    /// # Panics
    ///
    /// If the head wraps over the tail the behavior is undefined, that is,
    /// if `x` is out-of-range `[-(capacity() - len()), len()]`.
    ///
    /// If `-C debug-assertions=1` violating this pre-condition `panic!`s.
    ///
    /// # Unsafe
    ///
    /// It does not `drop` nor initialize elements, it just moves where the
    /// tail of the deque points to within the allocated buffer.
    #[inline]
    pub unsafe fn move_head_unchecked(&mut self, x: isize) {
        // Make sure that the head does not wrap over the tail:
        debug_assert!(x >= -((self.capacity() - self.len()) as isize));
        debug_assert!(x <= self.len() as isize);
        let head = self.head() as isize;
        let mut new_head = head + x;
        let tail = self.tail() as isize;
        let cap = self.capacity();
        debug_assert!(new_head <= tail);
        debug_assert!(tail - new_head <= cap as isize);

        if intrinsics::unlikely(new_head < 0) {
            // If the new head is negative we shift the range by capacity to
            // move it towards the second mirrored memory region.
            debug_assert!(tail < cap as isize);
            new_head += cap as isize;
            debug_assert!(new_head >= 0);
            self.tail_ += cap;
        } else if new_head as usize > cap {
            // cannot panic because new_head >= 0
            // If the new head is larger than the capacity, we shift the range
            // by -capacity to move it towards the first mirrored
            // memory region.
            let cap = cap as isize;
            debug_assert!(tail >= cap);
            new_head -= cap; //~ suspicious_integer_wrap
            debug_assert!(new_head >= 0);
            self.tail_ -= cap;
        }

        self.head_ = new_head as usize;
        debug_assert!(self.len() as isize == (tail - head) - x);
        debug_assert!(self.head() <= self.tail());

        debug_assert!(self.tail() <= self.tail_upper_bound());
        debug_assert!(self.head() <= self.head_upper_bound());

        debug_assert!(self.head() != self.capacity());
    }
}

/// Is `p` in bounds of `s` ?
///
/// Does it point to an element of `s` ? That is, one past the end of `s` is
/// not in bounds.
fn in_bounds<T>(s: &[T], p: *mut T) -> bool {
    let p = p as usize;
    let s_begin = s.as_ptr() as usize;
    let s_end = s_begin + mem::size_of::<T>() * s.len();
    s_begin <= p && p < s_end
}

unsafe fn nonnull_raw_slice<T>(ptr: *mut T, len: usize) -> NonNull<[T]> {
    unsafe { NonNull::new_unchecked(slice::from_raw_parts_mut(ptr, len)) }
}

/// Number of required memory allocation units to hold `bytes`.
fn no_required_allocation_units(bytes: usize) -> usize {
    let ag = allocation_granularity();
    let r = ((bytes + ag - 1) / ag).max(1);
    let r = if r % 2 == 0 { r } else { r + 1 };
    debug_assert!(r * ag >= bytes);
    debug_assert!(r % 2 == 0);
    r
}

impl<T> ops::Deref for SliceDeque<T> {
    type Target = [T];
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T> ops::DerefMut for SliceDeque<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

/// Mirrored memory buffer of length `len`.
///
/// The buffer elements in range `[0, len/2)` are mirrored into the range
/// `[len/2, len)`.
pub struct Buffer<T> {
    /// Pointer to the first element in the buffer.
    ptr: NonNull<T>,
    /// Length of the buffer:
    ///
    /// * it is NOT always a multiple of 2
    /// * the elements in range `[0, len/2)` are mirrored into the range
    /// `[len/2, len)`.
    len: usize,
}

impl<T> Buffer<T> {
    /// Number of elements in the buffer.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Is the buffer empty?
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Pointer to the first element in the buffer.
    pub unsafe fn ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }

    /// Interprets contents as a slice.
    ///
    /// Warning: Some memory might be uninitialized.
    pub unsafe fn as_slice(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len()) }
    }

    /// Interprets contents as a mut slice.
    ///
    /// Warning: Some memory might be uninitialized.
    pub unsafe fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len()) }
    }

    /// Creates a new empty `Buffer`.
    pub fn new() -> Self {
        // Zero-sized elements are not supported yet:
        assert!(mem::size_of::<T>() > 0);
        // Here `ptr` is initialized to a magic value but `len == 0`
        // will ensure that it is never dereferenced in this state.
        Self {
            ptr: NonNull::dangling(),
            len: 0,
        }
    }
    /// Total number of bytes in the buffer.
    pub fn size_in_bytes(len: usize) -> usize {
        let v = no_required_allocation_units(len * mem::size_of::<T>()) * allocation_granularity();
        debug_assert!(
            v >= len * mem::size_of::<T>(),
            "len: {}, so<T>: {}, v: {}",
            len,
            mem::size_of::<T>(),
            v
        );
        v
    }

    /// Create a mirrored buffer containing `len` `T`s where the first half of
    /// the buffer is mirrored into the second half.
    pub fn uninitialized(len: usize) -> Result<Self, AllocError> {
        // Zero-sized types are not supported yet:
        assert!(mem::size_of::<T>() > 0);
        // The alignment requirements of `T` must be smaller than the
        // allocation granularity.
        assert!(mem::align_of::<T>() <= allocation_granularity());
        // To split the buffer in two halfs the number of elements must be a
        // multiple of two, and greater than zero to be able to mirror
        // something.
        if len == 0 {
            return Ok(Self::new());
        }
        assert!(len % 2 == 0);

        // How much memory we need:
        let alloc_size = Self::size_in_bytes(len);
        debug_assert!(alloc_size > 0);
        debug_assert!(alloc_size % 2 == 0);
        debug_assert!(alloc_size % allocation_granularity() == 0);
        debug_assert!(alloc_size >= len * mem::size_of::<T>());

        let ptr = allocate_mirrored(alloc_size)?;
        Ok(Self {
            ptr: unsafe { NonNull::new_unchecked(ptr as *mut T) },
            len: alloc_size / mem::size_of::<T>(),
            // Note: len is not a multiple of two: debug_assert!(len % 2 == 0);
        })
    }
}

impl<T> Drop for Buffer<T> {
    fn drop(&mut self) {
        if self.is_empty() {
            return;
        }

        let buffer_size_in_bytes = Self::size_in_bytes(self.len());
        let first_half_ptr = self.ptr.as_ptr() as *mut u8;
        unsafe { deallocate_mirrored(first_half_ptr, buffer_size_in_bytes) };
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
mod mirrored {
    //! Non-racy linux-specific mirrored memory allocation.
    use libc::{
        _SC_PAGESIZE, ENOSYS, MAP_FAILED, MAP_FIXED, MAP_SHARED, PROT_READ, PROT_WRITE,
        SYS_memfd_create, c_char, c_int, c_long, c_uint, c_void, close, ftruncate, mkstemp, mmap,
        munmap, off_t, size_t, syscall, sysconf,
    };

    #[cfg(target_os = "android")]
    use libc::__errno;
    #[cfg(not(target_os = "android"))]
    use libc::__errno_location;

    use super::{AllocError, ptr};

    /// [`memfd_create`] - create an anonymous file
    ///
    /// [`memfd_create`]: http://man7.org/linux/man-pages/man2/memfd_create.2.html
    fn memfd_create(name: *const c_char, flags: c_uint) -> c_long {
        unsafe { syscall(SYS_memfd_create, name, flags) }
    }

    /// Returns the size of a memory allocation unit.
    ///
    /// In Linux-like systems this equals the page-size.
    pub fn allocation_granularity() -> usize {
        unsafe { sysconf(_SC_PAGESIZE) as usize }
    }

    /// Reads `errno`.
    fn errno() -> c_int {
        #[cfg(not(target_os = "android"))]
        unsafe {
            *__errno_location()
        }
        #[cfg(target_os = "android")]
        unsafe {
            *__errno()
        }
    }

    /// Allocates an uninitialzied buffer that holds `size` bytes, where
    /// the bytes in range `[0, size / 2)` are mirrored into the bytes in
    /// range `[size / 2, size)`.
    ///
    /// On Linux the algorithm is as follows:
    ///
    /// * 1. Allocate a memory-mapped file containing `size / 2` bytes.
    /// * 2. Map the file into `size` bytes of virtual memory.
    /// * 3. Map the file into the last `size / 2` bytes of the virtual memory
    /// region      obtained in step 2.
    ///
    /// This algorithm doesn't have any races.
    ///
    /// # Panics
    ///
    /// If `size` is zero or `size / 2` is not a multiple of the
    /// allocation granularity.
    pub fn allocate_mirrored(size: usize) -> Result<*mut u8, AllocError> {
        unsafe {
            let half_size = size / 2;
            assert!(size != 0);
            assert!(half_size % allocation_granularity() == 0);

            // create temporary file
            let mut fname = *b"/tmp/slice_deque_fileXXXXXX\0";
            let mut fd: c_long = memfd_create(fname.as_mut_ptr() as *mut c_char, 0);
            if fd == -1 && errno() == ENOSYS {
                // memfd_create is not implemented, use mkstemp instead:
                fd = c_long::from(mkstemp(fname.as_mut_ptr() as *mut c_char));
            }
            if fd == -1 {
                print_error("memfd_create failed");
                return Err(AllocError::Other);
            }
            let fd = fd as c_int;
            if ftruncate(fd, half_size as off_t) == -1 {
                print_error("ftruncate failed");
                if close(fd) == -1 {
                    print_error("@ftruncate: close failed");
                }
                return Err(AllocError::Oom);
            };

            // mmap memory
            let ptr = mmap(
                ptr::null_mut(),
                size,
                PROT_READ | PROT_WRITE,
                MAP_SHARED,
                fd,
                0,
            );
            if ptr == MAP_FAILED {
                print_error("@first: mmap failed");
                if close(fd) == -1 {
                    print_error("@first: close failed");
                }
                return Err(AllocError::Oom);
            }

            let ptr2 = mmap(
                (ptr as *mut u8).offset(half_size as isize) as *mut c_void,
                half_size,
                PROT_READ | PROT_WRITE,
                MAP_SHARED | MAP_FIXED,
                fd,
                0,
            );
            if ptr2 == MAP_FAILED {
                print_error("@second: mmap failed");
                if munmap(ptr, size as size_t) == -1 {
                    print_error("@second: munmap failed");
                }
                if close(fd) == -1 {
                    print_error("@second: close failed");
                }
                return Err(AllocError::Other);
            }

            if close(fd) == -1 {
                print_error("@success: close failed");
            }
            Ok(ptr as *mut u8)
        }
    }

    /// Deallocates the mirrored memory region at `ptr` of `size` bytes.
    ///
    /// # Unsafe
    ///
    /// `ptr` must have been obtained from a call to `allocate_mirrored(size)`,
    /// otherwise the behavior is undefined.
    ///
    /// # Panics
    ///
    /// If `size` is zero or `size / 2` is not a multiple of the
    /// allocation granularity, or `ptr` is null.
    pub unsafe fn deallocate_mirrored(ptr: *mut u8, size: usize) {
        assert!(!ptr.is_null());
        assert!(size != 0);
        assert!(size % allocation_granularity() == 0);
        if munmap(ptr as *mut c_void, size as size_t) == -1 {
            print_error("deallocate munmap failed");
        }
    }

    /// Prints last os error at `location`.
    #[cfg(all(debug_assertions, feature = "use_std"))]
    fn print_error(location: &str) {
        eprintln!(
            "Error at {}: {}",
            location,
            ::std::io::Error::last_os_error()
        );
    }

    /// Prints last os error at `location`.
    #[cfg(not(all(debug_assertions, feature = "use_std")))]
    fn print_error(_location: &str) {}
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod mirrored {
    //! Implements the allocator hooks on top of mach.
    extern crate mach;

    use super::mem;

    use mach::boolean::boolean_t;
    use mach::kern_return::*;
    use mach::mach_types::mem_entry_name_port_t;
    use mach::memory_object_types::{memory_object_offset_t, memory_object_size_t};
    use mach::traps::mach_task_self;
    use mach::vm::{
        mach_make_memory_entry_64, mach_vm_allocate, mach_vm_deallocate, mach_vm_remap,
    };
    use mach::vm_inherit::VM_INHERIT_NONE;
    use mach::vm_prot::{VM_PROT_READ, VM_PROT_WRITE, vm_prot_t};
    use mach::vm_statistics::{VM_FLAGS_ANYWHERE, VM_FLAGS_FIXED};
    use mach::vm_types::mach_vm_address_t;

    use super::AllocError;

    /// TODO: not exposed by the mach crate
    const VM_FLAGS_OVERWRITE: ::libc::c_int = 0x4000_i32;

    /// Returns the size of an allocation unit.
    ///
    /// In `MacOSX` this equals the page size.
    pub fn allocation_granularity() -> usize {
        unsafe { mach::vm_page_size::vm_page_size as usize }
    }

    /// Allocates an uninitialzied buffer that holds `size` bytes, where
    /// the bytes in range `[0, size / 2)` are mirrored into the bytes in
    /// range `[size / 2, size)`.
    ///
    /// On Macos X the algorithm is as follows:
    ///
    /// * 1. Allocate twice the memory (`size` bytes)
    /// * 2. Deallocate the second half (bytes in range `[size / 2, 0)`)
    /// * 3. Race condition: mirror bytes of the first half into the second
    /// half.
    ///
    /// If we get a race (e.g. because some other process allocates to the
    /// second half) we release all the resources (we need to deallocate the
    /// memory) and try again (up to a maximum of `MAX_NO_ALLOC_ITERS` times).
    ///
    /// # Panics
    ///
    /// If `size` is zero or `size / 2` is not a multiple of the
    /// allocation granularity.
    pub fn allocate_mirrored(size: usize) -> Result<*mut u8, AllocError> {
        unsafe {
            assert!(size != 0);
            let half_size = size / 2;
            assert!(half_size % allocation_granularity() == 0);

            let task = mach_task_self();

            // Allocate memory to hold the whole buffer:
            let mut addr: mach_vm_address_t = 0;
            let r: kern_return_t = mach_vm_allocate(
                task,
                &mut addr as *mut mach_vm_address_t,
                size as u64,
                VM_FLAGS_ANYWHERE,
            );
            if r != KERN_SUCCESS {
                // If the first allocation fails, there is nothing to
                // deallocate and we can just fail to allocate:
                print_error("initial alloc", r);
                return Err(AllocError::Oom);
            }
            debug_assert!(addr != 0);

            // Set the size of the first half to size/2:
            let r: kern_return_t = mach_vm_allocate(
                task,
                &mut addr as *mut mach_vm_address_t,
                half_size as u64,
                VM_FLAGS_FIXED | VM_FLAGS_OVERWRITE,
            );
            if r != KERN_SUCCESS {
                // If the first allocation fails, there is nothing to
                // deallocate and we can just fail to allocate:
                print_error("first half alloc", r);
                return Err(AllocError::Other);
            }

            // Get an object handle to the first memory region:
            let mut memory_object_size = half_size as memory_object_size_t;
            let mut object_handle: mem_entry_name_port_t = unsafe { mem::uninitialized() };
            let parent_handle: mem_entry_name_port_t = 0;
            let r: kern_return_t = mach_make_memory_entry_64(
                task,
                &mut memory_object_size as *mut memory_object_size_t,
                addr as memory_object_offset_t,
                VM_PROT_READ | VM_PROT_WRITE,
                &mut object_handle as *mut mem_entry_name_port_t,
                parent_handle,
            );

            if r != KERN_SUCCESS {
                // If making the memory entry fails we should deallocate the first
                // allocation:
                print_error("make memory entry", r);
                if dealloc(addr as *mut u8, size).is_err() {
                    panic!("failed to deallocate after error");
                }
                return Err(AllocError::Other);
            }

            // Map the first half to the second half using the object handle:
            let mut to = (addr as *mut u8).add(half_size) as mach_vm_address_t;
            let mut current_prot: vm_prot_t = unsafe { mem::uninitialized() };
            let mut out_prot: vm_prot_t = unsafe { mem::uninitialized() };
            let r: kern_return_t = mach_vm_remap(
                task,
                &mut to as *mut mach_vm_address_t,
                half_size as u64,
                /* mask: */ 0,
                VM_FLAGS_FIXED | VM_FLAGS_OVERWRITE,
                task,
                addr,
                /* copy: */ 0 as boolean_t,
                &mut current_prot as *mut vm_prot_t,
                &mut out_prot as *mut vm_prot_t,
                VM_INHERIT_NONE,
            );

            if r != KERN_SUCCESS {
                print_error("map first to second half", r);
                // If making the memory entry fails we deallocate all the memory
                if dealloc(addr as *mut u8, size).is_err() {
                    panic!("failed to deallocate after error");
                }
                return Err(AllocError::Other);
            }

            // TODO: object_handle is leaked here. Investigate whether this is ok.

            Ok(addr as *mut u8)
        }
    }

    /// Deallocates the mirrored memory region at `ptr` of `size` bytes.
    ///
    /// # Unsafe
    ///
    /// `ptr` must have been obtained from a call to `allocate_mirrored(size)`,
    /// otherwise the behavior is undefined.
    ///
    /// # Panics
    ///
    /// If `size` is zero or `size / 2` is not a multiple of the
    /// allocation granularity, or `ptr` is null.
    pub unsafe fn deallocate_mirrored(ptr: *mut u8, size: usize) {
        assert!(!ptr.is_null());
        assert!(size != 0);
        assert!(size % allocation_granularity() == 0);
        dealloc(ptr, size).expect("deallocating mirrored buffer failed");
    }

    /// Tries to deallocates `size` bytes of memory starting at `ptr`.
    ///
    /// # Unsafety
    ///
    /// The `ptr` must have been obtained from a previous call to `alloc` and point
    /// to a memory region containing at least `size` bytes.
    ///
    /// # Panics
    ///
    /// If `size` is zero or not a multiple of the `allocation_granularity`, or if
    /// `ptr` is null.
    unsafe fn dealloc(ptr: *mut u8, size: usize) -> Result<(), ()> {
        assert!(size != 0);
        assert!(size % allocation_granularity() == 0);
        assert!(!ptr.is_null());
        let addr = ptr as mach_vm_address_t;
        let r: kern_return_t = mach_vm_deallocate(mach_task_self(), addr, size as u64);
        if r != KERN_SUCCESS {
            print_error("dealloc", r);
            return Err(());
        }
        Ok(())
    }

    /// Prints last os error at `location`.
    #[cfg(not(all(debug_assertions, feature = "use_std")))]
    fn print_error(_msg: &str, _code: kern_return_t) {}

    /// Prints last os error at `location`.
    #[cfg(all(debug_assertions, feature = "use_std"))]
    fn print_error(msg: &str, code: kern_return_t) {
        eprintln!("ERROR at \"{}\": {}", msg, report_error(code));
    }

    /// Maps a vm `kern_return_t` to an error string.
    #[cfg(all(debug_assertions, feature = "use_std"))]
    fn report_error(error: kern_return_t) -> &'static str {
        use mach::kern_return::*;
        match error {
            KERN_ABORTED => "KERN_ABORTED",
            KERN_ALREADY_IN_SET => "KERN_ALREADY_IN_SET",
            KERN_ALREADY_WAITING => "KERN_ALREADY_WAITING",
            KERN_CODESIGN_ERROR => "KERN_CODESIGN_ERROR",
            KERN_DEFAULT_SET => "KERN_DEFAULT_SET",
            KERN_EXCEPTION_PROTECTED => "KERN_EXCEPTION_PROTECTED",
            KERN_FAILURE => "KERN_FAILURE",
            KERN_INVALID_ADDRESS => "KERN_INVALID_ADDRESS",
            KERN_INVALID_ARGUMENT => "KERN_INVALID_ARGUMENT",
            KERN_INVALID_CAPABILITY => "KERN_INVALID_CAPABILITY",
            KERN_INVALID_HOST => "KERN_INVALID_HOST",
            KERN_INVALID_LEDGER => "KERN_INVALID_LEDGER",
            KERN_INVALID_MEMORY_CONTROL => "KERN_INVALID_MEMORY_CONTROL",
            KERN_INVALID_NAME => "KERN_INVALID_NAME",
            KERN_INVALID_OBJECT => "KERN_INVALID_OBJECT",
            KERN_INVALID_POLICY => "KERN_INVALID_POLICY",
            KERN_INVALID_PROCESSOR_SET => "KERN_INVALID_PROCESSOR_SET",
            KERN_INVALID_RIGHT => "KERN_INVALID_RIGHT",
            KERN_INVALID_SECURITY => "KERN_INVALID_SECURITY",
            KERN_INVALID_TASK => "KERN_INVALID_TASK",
            KERN_INVALID_VALUE => "KERN_INVALID_VALUE",
            KERN_LOCK_OWNED => "KERN_LOCK_OWNED",
            KERN_LOCK_OWNED_SELF => "KERN_LOCK_OWNED_SELF",
            KERN_LOCK_SET_DESTROYED => "KERN_LOCK_SET_DESTROYED",
            KERN_LOCK_UNSTABLE => "KERN_LOCK_UNSTABLE",
            KERN_MEMORY_DATA_MOVED => "KERN_MEMORY_DATA_MOVED",
            KERN_MEMORY_ERROR => "KERN_MEMORY_ERROR",
            KERN_MEMORY_FAILURE => "KERN_MEMORY_FAILURE",
            KERN_MEMORY_PRESENT => "KERN_MEMORY_PRESENT",
            KERN_MEMORY_RESTART_COPY => "KERN_MEMORY_RESTART_COPY",
            KERN_NAME_EXISTS => "KERN_NAME_EXISTS",
            KERN_NODE_DOWN => "KERN_NODE_DOWN",
            KERN_NOT_DEPRESSED => "KERN_NOT_DEPRESSED",
            KERN_NOT_IN_SET => "KERN_NOT_IN_SET",
            KERN_NOT_RECEIVER => "KERN_NOT_RECEIVER",
            KERN_NOT_SUPPORTED => "KERN_NOT_SUPPORTED",
            KERN_NOT_WAITING => "KERN_NOT_WAITING",
            KERN_NO_ACCESS => "KERN_NO_ACCESS",
            KERN_NO_SPACE => "KERN_NO_SPACE",
            KERN_OPERATION_TIMED_OUT => "KERN_OPERATION_TIMED_OUT",
            KERN_POLICY_LIMIT => "KERN_POLICY_LIMIT",
            KERN_POLICY_STATIC => "KERN_POLICY_STATIC",
            KERN_PROTECTION_FAILURE => "KERN_PROTECTION_FAILURE",
            KERN_RESOURCE_SHORTAGE => "KERN_RESOURCE_SHORTAGE",
            KERN_RETURN_MAX => "KERN_RETURN_MAX",
            KERN_RIGHT_EXISTS => "KERN_RIGHT_EXISTS",
            KERN_RPC_CONTINUE_ORPHAN => "KERN_RPC_CONTINUE_ORPHAN",
            KERN_RPC_SERVER_TERMINATED => "KERN_RPC_SERVER_TERMINATED",
            KERN_RPC_TERMINATE_ORPHAN => "KERN_RPC_TERMINATE_ORPHAN",
            KERN_SEMAPHORE_DESTROYED => "KERN_SEMAPHORE_DESTROYED",
            KERN_SUCCESS => "KERN_SUCCESS",
            KERN_TERMINATED => "KERN_TERMINATED",
            KERN_UREFS_OVERFLOW => "KERN_UREFS_OVERFLOW",
            v => {
                eprintln!("unknown kernel error: {}", v);
                "UNKNOWN_KERN_ERROR"
            }
        }
    }
}

#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios"
    ),)
))]
mod mirrored {
    //! Racy System V mirrored memory allocation.
    use super::{AllocError, mem};
    use libc::{
        _SC_PAGESIZE, IPC_CREAT, IPC_PRIVATE, IPC_RMID, MAP_FAILED, MAP_PRIVATE, PROT_NONE, c_int,
        c_void, mmap, munmap, shmat, shmctl, shmdt, shmget, shmid_ds, sysconf,
    };

    #[cfg(not(target_os = "macos"))]
    use libc::MAP_ANONYMOUS;

    #[cfg(target_os = "macos")]
    use libc::MAP_ANON as MAP_ANONYMOUS;

    /// Returns the size of an allocation unit.
    ///
    /// System V shared memory has the page size as its allocation unit.
    pub fn allocation_granularity() -> usize {
        unsafe { sysconf(_SC_PAGESIZE) as usize }
    }

    /// System V Shared Memory handle.
    struct SharedMemory {
        id: c_int,
    }

    /// Map of System V Shared Memory to an address in the process address space.
    struct MemoryMap(*mut c_void);

    impl SharedMemory {
        /// Allocates `size` bytes of inter-process shared memory.
        ///
        /// Return the handle to this shared memory.
        ///
        /// # Panics
        ///
        /// If `size` is zero or not a multiple of the allocation granularity.
        pub fn allocate(size: usize) -> Result<SharedMemory, AllocError> {
            assert!(size != 0);
            assert!(size % allocation_granularity() == 0);
            unsafe {
                let id = shmget(IPC_PRIVATE, size, IPC_CREAT | 448);
                if id == -1 {
                    print_error("shmget");
                    return Err(AllocError::Oom);
                }
                Ok(SharedMemory { id })
            }
        }

        /// Attaches System V shared memory to the memory address at `ptr` in the
        /// address space of the current process.
        ///
        /// # Panics
        ///
        /// If `ptr` is null.
        pub fn attach(&self, ptr: *mut c_void) -> Result<MemoryMap, AllocError> {
            unsafe {
                // note: the success of allocate guarantees `shm_id != -1`.
                assert!(!ptr.is_null());
                let r = shmat(self.id, ptr, 0);
                if r as isize == -1 {
                    print_error("shmat");
                    return Err(AllocError::Other);
                }
                let map = MemoryMap(ptr);
                if r != ptr {
                    print_error("shmat2");
                    // map is dropped here, freeing the memory.
                    return Err(AllocError::Other);
                }
                Ok(map)
            }
        }
    }

    impl Drop for SharedMemory {
        /// Deallocates the inter-process shared memory..
        fn drop(&mut self) {
            unsafe {
                // note: the success of allocate guarantees `shm_id != -1`.
                let r = shmctl(self.id, IPC_RMID, 0 as *mut shmid_ds);
                if r == -1 {
                    // TODO: unlikely
                    // This should never happen, but just in case:
                    print_error("shmctl");
                    panic!("freeing system V shared-memory failed");
                }
            }
        }
    }

    impl MemoryMap {
        /// Initializes a MemoryMap to `ptr`.
        ///
        /// # Panics
        ///
        /// If `ptr` is null.
        ///
        /// # Unsafety
        ///
        /// If `ptr` does not point to a memory map created using
        /// `SharedMemory::attach` that has not been dropped yet..
        unsafe fn from_raw(ptr: *mut c_void) -> MemoryMap {
            assert!(!ptr.is_null());
            MemoryMap(ptr)
        }
    }

    impl Drop for MemoryMap {
        fn drop(&mut self) {
            unsafe {
                // note: the success of SharedMemory::attach and MemoryMap::new
                // guarantee `!self.0.is_null()`.
                let r = shmdt(self.0);
                if r == -1 {
                    // TODO: unlikely
                    print_error("shmdt");
                    panic!("freeing system V memory map failed");
                }
            }
        }
    }

    /// Allocates `size` bytes of uninitialized memory, where the bytes in range
    /// `[0, size / 2)` are mirrored into the bytes in range `[size / 2, size)`.
    ///
    /// The algorithm using System V interprocess shared-memory is:
    ///
    /// * 1. Allocate `size / 2` of interprocess shared memory.
    /// * 2. Reserve `size` bytes of virtual memory using `mmap + munmap`.
    /// * 3. Attach the shared memory to the first and second half.
    ///
    /// There is a race between steps 2 and 3 because after unmapping the memory
    /// and before attaching the shared memory to it another process might use that
    /// memory.
    pub fn allocate_mirrored(size: usize) -> Result<*mut u8, AllocError> {
        unsafe {
            assert!(size != 0);
            let half_size = size / 2;
            assert!(half_size % allocation_granularity() == 0);

            // 1. Allocate interprocess shared memory
            let shm = SharedMemory::allocate(half_size)?;

            const MAX_NO_ITERS: i32 = 10;
            let mut counter = 0;
            let ptr = loop {
                counter += 1;
                if counter > MAX_NO_ITERS {
                    return Err(AllocError::Other);
                }

                // 2. Reserve virtual memory:
                let ptr = mmap(
                    0 as *mut c_void,
                    size,
                    PROT_NONE,
                    MAP_ANONYMOUS | MAP_PRIVATE,
                    -1,
                    0,
                );
                if ptr == MAP_FAILED {
                    print_error("mmap initial");
                    return Err(AllocError::Oom);
                }

                let ptr2 = (ptr as *mut u8).offset(half_size as isize) as *mut c_void;

                unmap(ptr, size).expect("unmap initial failed");

                // 3. Attach shared memory to virtual memory:
                let map0 = shm.attach(ptr);
                if map0.is_err() {
                    print_error("attach_shm first failed");
                    continue;
                }
                let map1 = shm.attach(ptr2);
                if map1.is_err() {
                    print_error("attach_shm second failed");
                    continue;
                }
                // On success we leak the maps to keep them alive.
                // On drop we rebuild the maps from ptr and ptr + half_size
                // to deallocate them.
                mem::forget(map0);
                mem::forget(map1);
                break ptr;
            };

            Ok(ptr as *mut u8)
        }
    }

    /// Deallocates the mirrored memory region at `ptr` of `size` bytes.
    ///
    /// # Unsafe
    ///
    /// `ptr` must have been obtained from a call to `allocate_mirrored(size)` and
    /// not have been previously deallocated. Otherwise the behavior is undefined.
    ///
    /// # Panics
    ///
    /// If `size` is zero or `size / 2` is not a multiple of the
    /// allocation granularity, or `ptr` is null.
    pub unsafe fn deallocate_mirrored(ptr: *mut u8, size: usize) {
        let ptr2 = ptr.offset(size as isize / 2);
        MemoryMap::from_raw(ptr as *mut c_void);
        MemoryMap::from_raw(ptr2 as *mut c_void);
    }

    /// Unmaps the memory region at `[ptr, ptr+size)`.
    unsafe fn unmap(ptr: *mut c_void, size: usize) -> Result<(), ()> {
        let r = munmap(ptr, size);
        if r == -1 {
            print_error("unmap");
            return Err(());
        }
        Ok(())
    }

    #[cfg(not(all(debug_assertions, feature = "use_std")))]
    fn print_error(_location: &str) {}

    #[cfg(all(debug_assertions, feature = "use_std"))]
    fn print_error(location: &str) {
        eprintln!(
            "Error at {}: {}",
            location,
            ::std::io::Error::last_os_error()
        );
    }
}

#[cfg(target_os = "windows")]
mod mirrored {
    //! Implements the allocator hooks on top of window's virtual alloc.

    use mem;

    use winapi::shared::basetsd::SIZE_T;
    use winapi::shared::minwindef::{BOOL, DWORD, LPCVOID, LPVOID};
    use winapi::shared::ntdef::LPCWSTR;
    use winapi::um::memoryapi::{
        CreateFileMappingW, FILE_MAP_ALL_ACCESS, MapViewOfFileEx, UnmapViewOfFile, VirtualAlloc,
        VirtualFree,
    };
    use winapi::um::winnt::{MEM_RELEASE, MEM_RESERVE, PAGE_NOACCESS, PAGE_READWRITE, SEC_COMMIT};

    use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
    use winapi::um::minwinbase::LPSECURITY_ATTRIBUTES;
    use winapi::um::sysinfoapi::{GetSystemInfo, LPSYSTEM_INFO, SYSTEM_INFO};

    pub use winapi::shared::ntdef::HANDLE;

    use AllocError;

    /// Returns the size of an allocation unit in bytes.
    ///
    /// In Windows calls to `VirtualAlloc` must specify a multiple of
    /// `SYSTEM_INFO::dwAllocationGranularity` bytes.
    ///
    /// FIXME: the allocation granularity should always be larger than the page
    /// size (64k vs 4k), so determining the page size here is not necessary.
    pub fn allocation_granularity() -> usize {
        unsafe {
            let mut system_info: SYSTEM_INFO = mem::uninitialized();
            GetSystemInfo(&mut system_info as LPSYSTEM_INFO);
            let allocation_granularity = system_info.dwAllocationGranularity as usize;
            let page_size = system_info.dwPageSize as usize;
            page_size.max(allocation_granularity)
        }
    }

    /// Allocates an uninitialzied buffer that holds `size` bytes, where
    /// the bytes in range `[0, size / 2)` are mirrored into the bytes in
    /// range `[size / 2, size)`.
    ///
    /// On Windows the algorithm is as follows:
    ///
    /// * 1. Allocate physical memory to hold `size / 2` bytes using a   memory
    ///   mapped file.
    /// * 2. Find a region of virtual memory large enough to hold `size`
    /// bytes (by allocating memory with `VirtualAlloc` and immediately
    /// freeing   it with `VirtualFree`).
    /// * 3. Race condition: map the physical memory to the two halves of the
    ///   virtual memory region.
    ///
    /// If we get a race (e.g. because some other process obtains memory in the
    /// memory region where we wanted to map our physical memory) we release
    /// the first portion of virtual memory if mapping succeeded and try
    /// again (up to a maximum of `MAX_NO_ALLOC_ITERS` times).
    ///
    /// # Panics
    ///
    /// If `size` is zero or `size / 2` is not a multiple of the
    /// allocation granularity.
    pub fn allocate_mirrored(size: usize) -> Result<*mut u8, AllocError> {
        /// Maximum number of attempts to allocate in case of a race condition.
        const MAX_NO_ALLOC_ITERS: usize = 10;
        unsafe {
            let half_size = size / 2;
            assert!(size != 0);
            assert!(half_size % allocation_granularity() == 0);

            let file_mapping = create_file_mapping(half_size)?;

            let mut no_iters = 0;
            let virt_ptr = loop {
                if no_iters > MAX_NO_ALLOC_ITERS {
                    // If we exceeded the number of iterations try to close the
                    // handle and error:
                    close_file_mapping(file_mapping).expect("freeing physical memory failed");
                    return Err(AllocError::Other);
                }

                // Find large enough virtual memory region (if this fails we are
                // done):
                let virt_ptr = reserve_virtual_memory(size)?;

                // Map the physical memory to the first half:
                if map_view_of_file(file_mapping, half_size, virt_ptr).is_err() {
                    // If this fails, there is nothing to free and we try again:
                    no_iters += 1;
                    continue;
                }

                // Map physical memory to the second half:
                if map_view_of_file(file_mapping, half_size, virt_ptr.offset(half_size as isize))
                    .is_err()
                {
                    // If this fails, we release the map of the first half and try
                    // again:
                    no_iters += 1;
                    if unmap_view_of_file(virt_ptr).is_err() {
                        // If unmapping fails try to close the handle and
                        // panic:
                        close_file_mapping(file_mapping).expect("freeing physical memory failed");
                        panic!("unmapping first half of memory failed")
                    }
                    continue;
                }

                // We are done
                break virt_ptr;
            };

            // Close the file handle, it will be released when all the memory is
            // unmapped:
            close_file_mapping(file_mapping).expect("closing file handle failed");

            Ok(virt_ptr)
        }
    }

    /// Deallocates the mirrored memory region at `ptr` of `size` bytes.
    ///
    /// # Unsafe
    ///
    /// `ptr` must have been obtained from a call to `allocate_mirrored(size)`,
    /// otherwise the behavior is undefined.
    ///
    /// # Panics
    ///
    /// If `size` is zero or `size / 2` is not a multiple of the
    /// allocation granularity, or `ptr` is null.
    pub unsafe fn deallocate_mirrored(ptr: *mut u8, size: usize) {
        assert!(!ptr.is_null());
        assert!(size != 0);
        assert!(size % allocation_granularity() == 0);
        // On "windows" we unmap the memory.
        let half_size = size / 2;
        unmap_view_of_file(ptr).expect("unmapping first buffer half failed");
        let second_half_ptr = ptr.offset(half_size as isize);
        unmap_view_of_file(second_half_ptr).expect("unmapping second buffer half failed");
    }

    /// Creates a file mapping able to hold `size` bytes.
    ///
    /// # Panics
    ///
    /// If `size` is zero or not a multiple of the `allocation_granularity`.
    fn create_file_mapping(size: usize) -> Result<HANDLE, AllocError> {
        unsafe {
            assert!(size != 0);
            assert!(size % allocation_granularity() == 0);
            let dw_maximum_size_low: DWORD = size as DWORD;
            let dw_maximum_size_high: DWORD =
                match (mem::size_of::<DWORD>(), mem::size_of::<usize>()) {
                    // If both sizes are equal, the size is passed in the lower
                    // half, so the higher 32-bits are zero
                    (4, 4) | (8, 8) => 0,
                    // If DWORD is 32 bit but usize is 64-bit, we pass the higher
                    // 32-bit of size:
                    (4, 8) => (size >> 32) as DWORD,
                    _ => unimplemented!(),
                };

            let h: HANDLE = CreateFileMappingW(
                /* hFile: */ INVALID_HANDLE_VALUE as HANDLE,
                /* lpAttributes: */ 0 as LPSECURITY_ATTRIBUTES,
                /* flProtect: */ PAGE_READWRITE | SEC_COMMIT as DWORD,
                /* dwMaximumSizeHigh: */ dw_maximum_size_high,
                /* dwMaximumSizeLow: */ dw_maximum_size_low,
                /* lpName: */ 0 as LPCWSTR,
            );

            if h.is_null() {
                let s = tiny_str!("create_file_mapping (with size: {})", size);
                print_error(s.as_str());
                return Err(AllocError::Oom);
            }
            Ok(h)
        }
    }

    /// Closes a file mapping.
    ///
    /// # Unsafety
    ///
    /// `file_mapping` must point to a valid file mapping created with
    /// `create_file_mapping`.
    ///
    /// # Panics
    ///
    /// If `file_mapping` is null.
    unsafe fn close_file_mapping(file_mapping: HANDLE) -> Result<(), ()> {
        assert!(!file_mapping.is_null());

        let r: BOOL = CloseHandle(file_mapping);
        if r == 0 {
            print_error("close_file_mapping");
            return Err(());
        }
        Ok(())
    }

    /// Reserves a virtual memory region able to hold `size` bytes.
    ///
    /// The Windows API has no way to do this, so... we allocate a `size`-ed region
    /// with `VirtualAlloc`, immediately free it afterwards with `VirtualFree` and
    /// hope that the region is still available when we try to map into it.
    ///
    /// # Panics
    ///
    /// If `size` is not a multiple of the `allocation_granularity`.
    fn reserve_virtual_memory(size: usize) -> Result<(*mut u8), AllocError> {
        unsafe {
            assert!(size != 0);
            assert!(size % allocation_granularity() == 0);

            let r: LPVOID = VirtualAlloc(
                /* lpAddress: */ 0 as LPVOID,
                /* dwSize: */ size as SIZE_T,
                /* flAllocationType: */ MEM_RESERVE,
                /* flProtect: */ PAGE_NOACCESS,
            );

            if r.is_null() {
                print_error("reserve_virtual_memory(alloc failed)");
                return Err(AllocError::Oom);
            }

            let fr = VirtualFree(
                /* lpAddress: */ r,
                /* dwSize: */ 0 as SIZE_T,
                /* dwFreeType: */ MEM_RELEASE as DWORD,
            );
            if fr == 0 {
                print_error("reserve_virtual_memory(free failed)");
                return Err(AllocError::Other);
            }

            Ok(r as *mut u8)
        }
    }

    /// Maps `size` bytes of `file_mapping` to `address`.
    ///
    /// # Unsafety
    ///
    /// `file_mapping` must point to a valid file-mapping created with
    /// `create_file_mapping`.
    ///
    /// # Panics
    ///
    /// If `file_mapping` or `address` are null, or if `size` is zero or not a
    /// multiple of the allocation granularity of the system.
    unsafe fn map_view_of_file(
        file_mapping: HANDLE,
        size: usize,
        address: *mut u8,
    ) -> Result<(), ()> {
        assert!(!file_mapping.is_null());
        assert!(!address.is_null());
        assert!(size != 0);
        assert!(size % allocation_granularity() == 0);

        let r: LPVOID = MapViewOfFileEx(
            /* hFileMappingObject: */ file_mapping,
            /* dwDesiredAccess: */ FILE_MAP_ALL_ACCESS,
            /* dwFileOffsetHigh: */ 0 as DWORD,
            /* dwFileOffsetLow: */ 0 as DWORD,
            /* dwNumberOfBytesToMap: */ size as SIZE_T,
            /* lpBaseAddress: */ address as LPVOID,
        );
        if r.is_null() {
            print_error("map_view_of_file");
            return Err(());
        }
        debug_assert!(r == address as LPVOID);
        Ok(())
    }

    /// Unmaps the memory at `address`.
    ///
    /// # Unsafety
    ///
    /// If address does not point to a valid memory address previously mapped with
    /// `map_view_of_file`.
    ///
    /// # Panics
    ///
    /// If `address` is null.
    unsafe fn unmap_view_of_file(address: *mut u8) -> Result<(), ()> {
        assert!(!address.is_null());

        let r = UnmapViewOfFile(/* lpBaseAddress: */ address as LPCVOID);
        if r == 0 {
            print_error("unmap_view_of_file");
            return Err(());
        }
        Ok(())
    }

    /// Prints last os error at `location`.
    #[cfg(all(debug_assertions, feature = "use_std"))]
    fn print_error(location: &str) {
        eprintln!(
            "Error at {}: {}",
            location,
            ::std::io::Error::last_os_error()
        );
    }

    /// Prints last os error at `location`.
    #[cfg(not(all(debug_assertions, feature = "use_std")))]
    fn print_error(_location: &str) {}
}

fn main() {
    const C: [i16; 3] = [42; 3];

    let mut deque = SliceDeque::new();
    for _ in 0..918 {
        deque.push_front(C);
    }

    for _ in 0..237 {
        assert_eq!(deque.pop_front(), Some(C));
        assert!(!deque.is_empty());
        assert_eq!(*deque.back().unwrap(), C); // fails B != C
    }
}
