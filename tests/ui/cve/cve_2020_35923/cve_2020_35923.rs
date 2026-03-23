use std::cmp::Ordering;
use std::fmt;
use std::ops::{
    Add, AddAssign, Deref, Div, DivAssign, Mul, MulAssign, Rem, RemAssign, Sub, SubAssign,
};
use std::panic;

pub trait Float:
    Copy
    + PartialOrd
    + Add<Output = Self>
    + Sub<Output = Self>
    + Div<Output = Self>
    + Mul<Output = Self>
    + Rem<Output = Self>
    + std::fmt::Debug
{
    fn is_nan(self) -> bool;
}

impl Float for f32 {
    fn is_nan(self) -> bool {
        self.is_nan()
    }
}

impl Float for f64 {
    fn is_nan(self) -> bool {
        self.is_nan()
    }
}

/// A wrapper around floats providing an implementation of Eq, Ord and Hash.
/// A NaN value cannot be stored in this type.
#[derive(PartialOrd, PartialEq, Debug, Default, Clone, Copy)]
pub struct NotNan<T>(T);

impl<T: Float> NotNan<T> {
    /// Create a NotNan value.
    ///
    /// Returns Err if val is NaN
    pub fn new(val: T) -> Result<Self, FloatIsNan> {
        match val {
            ref val if val.is_nan() => Err(FloatIsNan),
            val => Ok(NotNan(val)),
        }
    }

    /// Get the value out.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: Float> AsRef<T> for NotNan<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T: Float> Deref for NotNan<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<T: Float + PartialEq> Eq for NotNan<T> {}

/// Adds two NotNans.
///
/// Panics if the computation results in NaN
impl<T: Float> Add for NotNan<T> {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        self + other.0
    }
}

/// Adds a float directly.
///
/// Panics if the provided value is NaN or the computation results in NaN
impl<T: Float> Add<T> for NotNan<T> {
    type Output = Self;

    fn add(self, other: T) -> Self {
        NotNan::new(self.0 + other).expect("Addition resulted in NaN")
    }
}

impl<T: Float + AddAssign> AddAssign for NotNan<T> {
    fn add_assign(&mut self, other: Self) {
        *self += other.0;
    }
}

/// Adds a float directly.
///
/// Panics if the provided value is NaN.
impl<T: Float + AddAssign> AddAssign<T> for NotNan<T> {
    fn add_assign(&mut self, other: T) {
        self.0 += other;
        assert!(!self.0.is_nan(), "Addition resulted in NaN");
    }
}

impl<T: Float> Sub for NotNan<T> {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        self - other.0
    }
}

/// Subtracts a float directly.
///
/// Panics if the provided value is NaN or the computation results in NaN
impl<T: Float> Sub<T> for NotNan<T> {
    type Output = Self;

    fn sub(self, other: T) -> Self {
        NotNan::new(self.0 - other).expect("Subtraction resulted in NaN")
    }
}

impl<T: Float + SubAssign> SubAssign for NotNan<T> {
    fn sub_assign(&mut self, other: Self) {
        *self -= other.0
    }
}

/// Subtracts a float directly.
///
/// Panics if the provided value is NaN or the computation results in NaN
impl<T: Float + SubAssign> SubAssign<T> for NotNan<T> {
    fn sub_assign(&mut self, other: T) {
        self.0 -= other;
        assert!(!self.0.is_nan(), "Subtraction resulted in NaN");
    }
}

impl<T: Float> Mul for NotNan<T> {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        self * other.0
    }
}

/// Multiplies a float directly.
///
/// Panics if the provided value is NaN or the computation results in NaN
impl<T: Float> Mul<T> for NotNan<T> {
    type Output = Self;

    fn mul(self, other: T) -> Self {
        NotNan::new(self.0 * other).expect("Multiplication resulted in NaN")
    }
}

impl<T: Float + MulAssign> MulAssign for NotNan<T> {
    fn mul_assign(&mut self, other: Self) {
        *self *= other.0
    }
}

/// Multiplies a float directly.
///
/// Panics if the provided value is NaN.
impl<T: Float + MulAssign> MulAssign<T> for NotNan<T> {
    fn mul_assign(&mut self, other: T) {
        self.0 *= other;
        assert!(!self.0.is_nan(), "Multiplication resulted in NaN");
    }
}

impl<T: Float> Div for NotNan<T> {
    type Output = Self;

    fn div(self, other: Self) -> Self {
        self / other.0
    }
}

/// Divides a float directly.
///
/// Panics if the provided value is NaN or the computation results in NaN
impl<T: Float> Div<T> for NotNan<T> {
    type Output = Self;

    fn div(self, other: T) -> Self {
        NotNan::new(self.0 / other).expect("Division resulted in NaN")
    }
}

impl<T: Float + DivAssign> DivAssign for NotNan<T> {
    fn div_assign(&mut self, other: Self) {
        *self /= other.0;
    }
}

/// Divides a float directly.
///
/// Panics if the provided value is NaN or the computation results in NaN
impl<T: Float + DivAssign> DivAssign<T> for NotNan<T> {
    fn div_assign(&mut self, other: T) {
        self.0 /= other;
        assert!(!self.0.is_nan(), "Division resulted in NaN");
    }
}

impl<T: Float> Rem for NotNan<T> {
    type Output = Self;

    fn rem(self, other: Self) -> Self {
        self % other.0
    }
}

/// Calculates `%` with a float directly.
///
/// Panics if the provided value is NaN or the computation results in NaN
impl<T: Float> Rem<T> for NotNan<T> {
    type Output = Self;

    fn rem(self, other: T) -> Self {
        NotNan::new(self.0 % other).expect("Rem resulted in NaN")
    }
}

impl<T: Float + RemAssign> RemAssign for NotNan<T> {
    fn rem_assign(&mut self, other: Self) {
        *self %= other.0
    }
}

/// Calculates `%=` with a float directly.
///
/// Panics if the provided value is NaN or the computation results in NaN
impl<T: Float + RemAssign> RemAssign<T> for NotNan<T> {
    fn rem_assign(&mut self, other: T) {
        self.0 %= other;
        assert!(!self.0.is_nan(), "Rem resulted in NaN");
    }
}

impl<T: Float> Ord for NotNan<T> {
    fn cmp(&self, other: &NotNan<T>) -> Ordering {
        match self.partial_cmp(&other) {
            //~^ unreachable_unchecked_depend_on_option
            //~| unreachable_unchecked_depend_on_option
            //~| unreachable_unchecked_depend_on_option
            //~| unreachable_unchecked_depend_on_option
            Some(ord) => ord,
            None => unsafe { std::hint::unreachable_unchecked() },
        }
    }
}

/// An error indicating an attempt to construct NotNan from a NaN
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct FloatIsNan;

impl fmt::Display for FloatIsNan {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "NotNan constructed with NaN")
    }
}

fn not_nan<T: Float>(x: T) -> NotNan<T> {
    NotNan::new(x).unwrap()
}

fn main() {
    let catch_op = |mut num, op: fn(&mut NotNan<_>)| {
        let mut num_ref = panic::AssertUnwindSafe(&mut num);
        let _ = panic::catch_unwind(move || op(*num_ref));
        num
    };

    let a = catch_op(not_nan(0.0), |a| *a /= 0.0);
    assert_eq!(a.cmp(&a), std::cmp::Ordering::Equal);

    assert!(!catch_op(not_nan(f32::INFINITY), |a| *a += f32::NEG_INFINITY).is_nan());
    assert!(!catch_op(not_nan(f32::INFINITY), |a| *a -= f32::INFINITY).is_nan());
    assert!(!catch_op(not_nan(0.0), |a| *a *= f32::INFINITY).is_nan());
    assert!(!catch_op(not_nan(0.0), |a| *a /= 0.0).is_nan());
    assert!(!catch_op(not_nan(0.0), |a| *a %= 0.0).is_nan());
}
