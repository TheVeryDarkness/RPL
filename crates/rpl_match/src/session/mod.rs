mod bindings;
mod collect;
mod config;
mod csp;
mod multi_matched;
mod session;
mod slot;

pub use bindings::{BindingSnapshot, MetaBindings};
pub use collect::MatchCollectCtxt;
pub use config::{SessionConfig, DEFAULT_MAX_SESSION_RESULTS};
pub use csp::CspSolver;
pub use multi_matched::{MultiMatched, OwnedLintMatch, SessionLintTarget};
pub use session::MatchSession;
pub use slot::{
    collect_slot_descs, AdtSlotCandidate, AdtSlotDesc, CrateAdtItem, CrateFnItem, CrateItemIndex,
    FnMatchContext, FnSlotCandidate, FnSlotDesc, MatchSlot, SessionResult, SlotAssignment,
    SlotCandidate,
};
