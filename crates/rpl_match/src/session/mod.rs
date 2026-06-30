mod bindings;
mod collect;
mod config;
mod csp;
mod multi_matched;
mod session;
mod slot;

pub use bindings::{BindingSnapshot, MetaBindings};
pub use collect::MatchCollectCtxt;
pub use config::{DEFAULT_MAX_SESSION_RESULTS, SessionConfig};
pub use csp::CspSolver;
pub use multi_matched::{MultiMatched, OwnedLintMatch, SessionLintTarget};
pub use session::MatchSession;
pub use slot::{
    AdtSlotCandidate, AdtSlotDesc, CrateAdtItem, CrateFnItem, CrateItemIndex, FnMatchContext, FnSlotCandidate,
    FnSlotDesc, MatchSlot, SessionResult, SlotAssignment, SlotCandidate, collect_slot_descs,
};
