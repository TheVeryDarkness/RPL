#![feature(rustc_private)]
#![warn(unused_qualifications)]
extern crate rustc_data_structures;
extern crate rustc_errors;
extern crate rustc_fluent_macro;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_lint_defs;
extern crate rustc_macros;
extern crate rustc_middle;
extern crate rustc_session;
extern crate rustc_span;
#[macro_use]
extern crate tracing;
extern crate either;

rustc_fluent_macro::fluent_messages! { "../messages.en.ftl" }

use std::borrow::Cow;

use rpl_context::PatCtxt;
use rpl_meta::context::MetaContext;
use rustc_lint_defs::RegisteredTools;
use rustc_macros::{Diagnostic, LintDiagnostic};
use rustc_middle::ty::TyCtxt;
use rustc_middle::util::Providers;
use rustc_session::declare_tool_lint;
use rustc_span::symbol::Ident;

use crate::check::CheckFnCtxt;

#[cfg(feature = "timing")]
mod errors;
#[cfg(feature = "timing")]
pub use errors::{TIMING, Timing};
mod check;
mod check2;
mod utils;

declare_tool_lint! {
    /// The `rpl::error_found` lint detects an error.
    ///
    /// ### Example
    ///
    /// ```rust
    /// ```
    ///
    /// {{produces}}
    ///
    /// ### Explanation
    ///
    /// This lint detects an error.
    pub rpl::ERROR_FOUND,
    Deny,
    "detects an error"
}

#[derive(Diagnostic, LintDiagnostic)]
#[diag(rpl_driver_error_found_with_pattern)]
pub struct ErrorFound;

impl From<ErrorFound> for rustc_errors::DiagMessage {
    fn from(_: ErrorFound) -> Self {
        Self::Str(Cow::Borrowed("An error was found with input RPL pattern(s)"))
    }
}

pub fn provide(providers: &mut Providers) {
    providers.registered_tools = registered_tools;
}

fn registered_tools(tcx: TyCtxt<'_>, (): ()) -> RegisteredTools {
    let mut registered_tools = (rustc_interface::DEFAULT_QUERY_PROVIDERS.registered_tools)(tcx, ());
    registered_tools.insert(Ident::from_str("rpl"));
    registered_tools
}

pub fn check_crate<'tcx, 'pcx, 'mcx: 'pcx>(tcx: TyCtxt<'tcx>, pcx: PatCtxt<'pcx>, mctx: &'mcx MetaContext<'mcx>) {
    #[cfg(feature = "timing")]
    let start = std::time::Instant::now();

    pcx.add_parsed_patterns(mctx);

    #[cfg(feature = "timing")]
    {
        use rustc_hir::def_id::CrateNum;

        use crate::errors::TIMING;

        let time = start.elapsed().as_nanos().try_into().unwrap();
        let hir_id = rustc_hir::hir_id::CRATE_HIR_ID;
        let crate_name = tcx.crate_name(CrateNum::ZERO);
        tcx.emit_node_span_lint(
            TIMING,
            hir_id,
            tcx.hir().span(hir_id),
            Timing {
                time,
                stage: "add_parsed_patterns",
                crate_name,
            },
        );
    }

    #[cfg(feature = "timing")]
    let start = std::time::Instant::now();

    // _ = tcx.hir_crate_items(()).par_items(|item_id| {
    //     check_item(tcx, pcx, item_id);
    //     Ok(())
    // });

    // let mut check_ctxt = CheckFnCtxt::new(tcx, pcx);
    // tcx.hir().walk_toplevel_module(&mut check_ctxt);

    check2::walk2(tcx, pcx);

    rpl_utils::visit_crate(tcx);

    #[cfg(feature = "timing")]
    {
        use rustc_hir::def_id::CrateNum;

        use crate::errors::TIMING;

        let time = start.elapsed().as_nanos().try_into().unwrap();
        let hir_id = rustc_hir::hir_id::CRATE_HIR_ID;
        let crate_name = tcx.crate_name(CrateNum::ZERO);
        tcx.emit_node_span_lint(
            TIMING,
            hir_id,
            tcx.hir().span(hir_id),
            Timing {
                time,
                stage: "do_match",
                crate_name,
            },
        );
    }
}
