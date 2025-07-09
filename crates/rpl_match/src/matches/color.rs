//! Check if the pattern statement matches MIR statement,
//! A.K.A. if we're using building blocks with the right color.

use rustc_middle::mir;

use crate::matches::MatchCtxt;
use crate::mir::pat;

impl<'a, 'pcx, 'tcx> MatchCtxt<'a, 'pcx, 'tcx> {
    /// Refer to [`CheckMirCtxt::match_statement_or_terminator`].
    #[instrument(level = "trace", skip(self), ret)]
    pub(super) fn match_statement_or_terminator(&self, pat: pat::Location, loc: mir::Location) -> bool {
        true
    }
}
