use std::fmt;

use rustc_hir::def_id::LocalDefId;
use rustc_middle::mir;
use smallvec::SmallVec;

use crate::match2::matched::StatementMatch;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Location {
    def_id: LocalDefId,
    loc: mir::Location,
}

// It's sad that the `Location` (also `(LocalDefId, mir::Location)`) is 24 bytes, where there is
// some padding, summing up to 8 bytes, and it could be saved if we pack the `mir::Location` better.
static_assertions::const_assert_eq!(size_of::<LocalDefId>(), 4usize);
static_assertions::const_assert_eq!(size_of::<mir::BasicBlock>(), 4usize);
static_assertions::const_assert_eq!(size_of::<mir::Location>(), 16usize);
static_assertions::const_assert_eq!(size_of::<Location>(), 24usize);

/// A simple wrapper to track call stack during matching.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct WithCallStack<T>(
    /// the call stack, from innermost (callee) to outermost (caller)
    /// each entry consists of (call site def id, call site basic block)
    SmallVec<[Location; 1]>,
    /// the innermost function (where the value is defined)
    LocalDefId,
    /// the value being tracked
    T,
);
static_assertions::const_assert_eq!(size_of::<Location>(), 24usize);
static_assertions::const_assert_eq!(size_of::<WithCallStack<()>>(), 40usize);
static_assertions::const_assert_eq!(align_of::<WithCallStack<()>>(), 8usize);

impl<T> WithCallStack<T> {
    pub(crate) fn new_one(def_id: LocalDefId, value: T) -> Self {
        Self(SmallVec::new(), def_id, value)
    }
    pub(crate) fn push_call(&mut self, def_id: LocalDefId, loc: mir::Location) {
        self.0.push(Location { def_id, loc });
    }
    /// Get the definition id and the value.
    pub(crate) fn def(&self) -> (LocalDefId, &T) {
        (self.1, &self.2)
    }
    /// Get the bottom (outermost) function and its basic block if any.
    pub(crate) fn bottom(&self) -> (LocalDefId, Option<mir::Location>) {
        if let Some(Location { def_id, loc }) = self.0.first() {
            (*def_id, Some(*loc))
        } else {
            (self.1, None)
        }
    }
}
impl<T: Copy> WithCallStack<T> {
    /// Get the matched value.
    pub(crate) fn value(&self) -> T {
        self.2
    }
}

impl<T: fmt::Debug> fmt::Debug for WithCallStack<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.2)?;
        write!(f, " <- {:?}", self.1)?;
        for Location { def_id, loc } in &self.0 {
            write!(f, " <- {:?} @ {:?}", def_id, loc)?;
        }
        Ok(())
    }
}

impl WithCallStack<StatementMatch> {
    pub(crate) fn bottom_location(&self) -> (LocalDefId, Option<mir::Location>) {
        if let Some(Location { def_id, loc }) = self.0.first() {
            (*def_id, Some(*loc))
        } else if let StatementMatch::Location(loc) = &self.2 {
            (self.1, Some(*loc))
        } else {
            (self.1, None)
        }
    }
}
