//@check-pass: patched
struct Pixel<T> {
    channels: [T; 3],
}

impl<T> Pixel<T> {
    fn from_slice_mut(slice: &mut [T]) -> &mut Self {
        assert_eq!(slice.len(), 3);
        unsafe { &mut *(slice.as_mut_ptr() as *mut Self) }
    }
}

fn main() {
    let mut data = [0u8; 3];
    let mut pixel = Pixel::from_slice_mut(&mut data);
    pixel.channels[0] = 255;
}
