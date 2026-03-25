use rustc_hir::{FnHeader, intravisit};
use rustc_span::Ident;

pub(crate) fn fn_name<'tcx>(kind: intravisit::FnKind<'tcx>) -> (Option<Ident>, Option<FnHeader>) {
    match kind {
        intravisit::FnKind::ItemFn(name, _, fn_header) => (Some(name), Some(fn_header)),
        intravisit::FnKind::Method(name, fn_sig) => (Some(name), Some(fn_sig.header)),
        intravisit::FnKind::Closure => (None, None),
    }
}
