struct Pixel<T> {
    channels: [T; 3],
}

impl<T> Pixel<T> {
    fn ptr_from_slice_mut(slice: &mut [T]) -> *mut Self {
        let ptr = slice.as_ptr();
        ptr as *mut Self
    }
    fn from_slice_mut(slice: &mut [T]) -> &mut Self {
        assert_eq!(slice.len(), 3);
        let ptr = Self::ptr_from_slice_mut(slice);
        unsafe { &mut *ptr } //~mut_ref_from_const_ptr
    }
}

fn main() {
    let mut data = [0u8; 3];
    let mut pixel = Pixel::from_slice_mut(&mut data);
    pixel.channels[0] = 255;
}
