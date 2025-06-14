use std::collections::HashSet;

use rpl_context::PatCtxt;
use rpl_mir::{CheckMirCtxt, pat};
use rustc_hir as hir;
use rustc_hir::def_id::LocalDefId;
use rustc_hir::intravisit::{self, Visitor};
use rustc_middle::hir::nested_filter::All;
use rustc_middle::ty::TyCtxt;
use rustc_span::{Span, Symbol};

use crate::lints::DROP_UNINIT_VALUE;

#[instrument(level = "info", skip_all)]
pub fn check_item(tcx: TyCtxt<'_>, pcx: PatCtxt<'_>, item_id: hir::ItemId) {
    let item = tcx.hir().item(item_id);
    // let def_id = item_id.owner_id.def_id;
    let mut check_ctxt = CheckFnCtxt { tcx, pcx };
    check_ctxt.visit_item(item);
}

struct CheckFnCtxt<'pcx, 'tcx> {
    tcx: TyCtxt<'tcx>,
    pcx: PatCtxt<'pcx>,
}

impl<'tcx> Visitor<'tcx> for CheckFnCtxt<'_, 'tcx> {
    type NestedFilter = All;
    fn nested_visit_map(&mut self) -> Self::Map {
        self.tcx.hir()
    }

    #[instrument(level = "debug", skip_all, fields(?item.owner_id))]
    fn visit_item(&mut self, item: &'tcx hir::Item<'tcx>) -> Self::Result {
        match item.kind {
            hir::ItemKind::Trait(hir::IsAuto::No, hir::Safety::Safe, ..)
            | hir::ItemKind::Impl(_)
            | hir::ItemKind::Fn { .. } => {},
            _ => return,
        }
        intravisit::walk_item(self, item);
    }

    #[instrument(level = "info", skip_all, fields(?def_id))]
    fn visit_fn(
        &mut self,
        kind: intravisit::FnKind<'tcx>,
        decl: &'tcx hir::FnDecl<'tcx>,
        body_id: hir::BodyId,
        _span: Span,
        def_id: LocalDefId,
    ) -> Self::Result {
        if self.tcx.is_mir_available(def_id) {
            let body = self.tcx.optimized_mir(def_id);
            let pattern = pattern_drop_unit_value(self.pcx);
            let mut records = HashSet::new();
            for matches in CheckMirCtxt::new(self.tcx, self.pcx, body, pattern.pattern, pattern.fn_pat).check() {
                let record = (pattern.drop, pattern.alloc, pattern.ptr, pattern.assign);
                if records.contains(&record) {
                    continue;
                }
                records.insert(record);
                let drop = matches[pattern.drop].span_no_inline(body);
                let alloc = matches[pattern.alloc].span_no_inline(body);
                let ptr = matches[pattern.ptr].span_no_inline(body);
                let assign = matches[pattern.assign].span_no_inline(body);
                self.tcx.emit_node_span_lint(
                    DROP_UNINIT_VALUE,
                    self.tcx.local_def_id_to_hir_id(def_id),
                    drop,
                    crate::errors::DropUninitValue {
                        drop,
                        alloc,
                        ptr,
                        assign,
                    },
                );
            }
        }
        intravisit::walk_fn(self, kind, decl, body_id, def_id);
    }
}

struct Pattern<'pcx> {
    pattern: &'pcx pat::Pattern<'pcx>,
    fn_pat: &'pcx pat::Fn<'pcx>,
    drop: pat::Location,
    alloc: pat::Location,
    ptr: pat::Location,
    assign: pat::Location,
}

#[rpl_macros::pattern_def]
fn pattern_drop_unit_value(pcx: PatCtxt<'_>) -> Pattern<'_> {
    let alloc;
    let ptr;
    let assign;
    let drop;
    let pattern = rpl! {
        #[meta($T:ty)]
        fn $pattern (..) -> _ = mir! {
            let $size: usize = _;
            let $align: usize = _;
            #[export(alloc)]
            let $alloc_ptr: *mut u8 = alloc::alloc::__rust_alloc(move $size, move $align);
            #[export(ptr)]
            let $raw_ptr: *mut $T = _; // FIXME: related to $alloc_ptr
            let $value: $T = _;
            #[export(drop)]
            drop((*$raw_ptr));
            #[export(assign)]
            (*$raw_ptr) = move $value;
        }
    };
    let fn_pat = pattern.fns.get_fn_pat(Symbol::intern("pattern")).unwrap();

    Pattern {
        pattern,
        fn_pat,
        drop,
        alloc,
        ptr,
        assign,
    }
}
