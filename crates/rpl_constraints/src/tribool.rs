#![allow(clippy::match_same_arms)]

use core::fmt;
use core::ops::{BitAnd, BitOr, Not};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TriBool {
    True,
    False,
    Unknown,
}

impl From<bool> for TriBool {
    fn from(b: bool) -> Self {
        if b { TriBool::True } else { TriBool::False }
    }
}

impl From<Option<bool>> for TriBool {
    fn from(opt: Option<bool>) -> Self {
        match opt {
            Some(true) => TriBool::True,
            Some(false) => TriBool::False,
            None => TriBool::Unknown,
        }
    }
}

impl From<TriBool> for Option<bool> {
    fn from(tb: TriBool) -> Self {
        match tb {
            TriBool::True => Some(true),
            TriBool::False => Some(false),
            TriBool::Unknown => None,
        }
    }
}

impl TryFrom<TriBool> for bool {
    type Error = ();
    fn try_from(tb: TriBool) -> Result<Self, Self::Error> {
        match tb {
            TriBool::True => Ok(true),
            TriBool::False => Ok(false),
            TriBool::Unknown => Err(()),
        }
    }
}

impl Not for TriBool {
    type Output = Self;
    fn not(self) -> Self {
        match self {
            TriBool::True => TriBool::False,
            TriBool::False => TriBool::True,
            TriBool::Unknown => TriBool::Unknown,
        }
    }
}

/// Kleene K3 "and" truth table:
/// T ∧ T = T; F ∧ _ = F; U ∧ T = U; U ∧ U = U
impl BitAnd for TriBool {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        match (self, rhs) {
            (TriBool::False, _) | (_, TriBool::False) => TriBool::False,
            (TriBool::True, TriBool::True) => TriBool::True,
            _ => TriBool::Unknown,
        }
    }
}

/// Kleene K3 "or" truth table:
/// F ∨ F = F; T ∨ _ = T; U ∨ F = U; U ∨ U = U
impl BitOr for TriBool {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        match (self, rhs) {
            (TriBool::True, _) | (_, TriBool::True) => TriBool::True,
            (TriBool::False, TriBool::False) => TriBool::False,
            _ => TriBool::Unknown,
        }
    }
}

impl fmt::Display for TriBool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TriBool::True => "true",
            TriBool::False => "false",
            TriBool::Unknown => "unknown",
        };
        f.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo() {
        let t = TriBool::True;
        let f = TriBool::False;
        let u = TriBool::Unknown;

        assert_eq!(!t, f);
        assert_eq!(t & u, u);
        assert_eq!(t | u, t);
        assert_eq!(u & f, f);
        assert_eq!(u | f, u);
        assert_eq!(TriBool::from(Some(true)), t);
        assert_eq!(Option::<bool>::from(u), None);
    }
}
