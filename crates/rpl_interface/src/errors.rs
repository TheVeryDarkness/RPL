use rustc_macros::{Diagnostic, LintDiagnostic};
use rustc_session::declare_tool_lint;
use rustc_span::Symbol;

declare_tool_lint!(
    pub rpl_interface::TIMING,
    Warn,
    "timing information for RPL interface"
);

#[derive(Diagnostic, LintDiagnostic)]
#[diag(rpl_interface_timing)]
pub(crate) struct Timing {
    /// Used time in nanoseconds
    pub(crate) time: u64,
    pub(crate) crate_name: Symbol,
}
