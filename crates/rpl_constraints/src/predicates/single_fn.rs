use rpl_resolve::{PatItemKind, def_path_res};
use rustc_hir::def::{DefKind, Res};
use rustc_hir::def_id::{DefId, LocalDefId};
use rustc_middle::mir::{self, Operand, TerminatorKind};
use rustc_middle::ty::{self, TyCtxt};
use rustc_span::Symbol;

pub type SingleFnPredsFnPtr = for<'tcx> fn(tcx: TyCtxt<'tcx>, def_id: LocalDefId) -> bool;

/// Check if self_ty's trait bounds are all safe.
#[instrument(level = "debug", skip(tcx), ret)]
pub fn requires_monomorphization<'tcx>(tcx: TyCtxt<'tcx>, def_id: LocalDefId) -> bool {
    tcx.generics_of(def_id).requires_monomorphization(tcx)
}

/// Returns true when `def_id` is a function that runs outside the `main` lifetime,
/// e.g. a `#[ctor]`/`#[dtor]` function after `ctor` crate macro expansion.
#[instrument(level = "debug", skip(tcx), ret)]
pub fn runs_outside_main<'tcx>(tcx: TyCtxt<'tcx>, def_id: LocalDefId) -> bool {
    is_invoked_from_ctor_crate_hook(tcx, def_id)
        || mir_has_ctor_exit_hook_call(tcx, def_id)
}

/// After `ctor` macro expansion, registration hooks call the user function:
/// - `#[ctor]`: `{name}::__CTOR_FUNCTION::__CTOR_FUNCTION_INNER` calls `{name}()`
/// - `#[dtor]`: `{name}::__dtor` calls `{name}()`
fn is_invoked_from_ctor_crate_hook<'tcx>(tcx: TyCtxt<'tcx>, def_id: LocalDefId) -> bool {
    let target = def_id.to_def_id();
    tcx.hir_crate_items(())
        .nested_bodies()
        .chain(tcx.hir_crate_items(()).definitions())
        .any(|hook| {
            if hook == def_id || !matches!(tcx.def_kind(hook), DefKind::Fn) || !tcx.is_mir_available(hook) {
                return false;
            }
            let path = tcx.def_path_str(hook);
            let is_ctor_hook = path.contains("__CTOR_FUNCTION_INNER");
            let is_dtor_hook = path.contains("__dtor");
            (is_ctor_hook || is_dtor_hook) && mir_has_zero_arg_call_to(tcx, hook, target)
        })
}

fn mir_has_ctor_exit_hook_call<'tcx>(tcx: TyCtxt<'tcx>, def_id: LocalDefId) -> bool {
    if !tcx.is_mir_available(def_id) {
        return false;
    }
    let body = tcx.optimized_mir(def_id);
    body.basic_blocks.iter().any(|bb_data| {
        let Some(term) = bb_data.terminator.as_ref() else {
            return false;
        };
        let TerminatorKind::Call { func, .. } = &term.kind else {
            return false;
        };
        let Some(callee) = callee_def_id(func) else {
            return false;
        };
        is_ctor_exit_hook(tcx, callee)
    })
}

fn mir_has_zero_arg_call_to<'tcx>(tcx: TyCtxt<'tcx>, caller: LocalDefId, callee: DefId) -> bool {
    let body = tcx.optimized_mir(caller);
    body.basic_blocks.iter().any(|bb_data| {
        let Some(term) = bb_data.terminator.as_ref() else {
            return false;
        };
        let TerminatorKind::Call { func, args, .. } = &term.kind else {
            return false;
        };
        args.is_empty() && callee_def_id(func) == Some(callee)
    })
}

fn callee_def_id<'tcx>(func: &Operand<'tcx>) -> Option<DefId> {
    let Operand::Constant(box mir::ConstOperand { const_, .. }) = func else {
        return None;
    };
    let ty::FnDef(def_id, _) = const_.ty().kind() else {
        return None;
    };
    Some(*def_id)
}

fn is_ctor_exit_hook<'tcx>(tcx: TyCtxt<'tcx>, callee: DefId) -> bool {
    for path in ["ctor::__support::at_binary_exit", "ctor::__support::at_library_exit"] {
        let symbols: Vec<Symbol> = path.split("::").map(Symbol::intern).collect();
        let resolved = def_path_res(tcx, &symbols, PatItemKind::Fn);
        if resolved.iter().any(|res| matches!(res, Res::Def(DefKind::Fn, id) if *id == callee)) {
            return true;
        }
    }
    false
}
