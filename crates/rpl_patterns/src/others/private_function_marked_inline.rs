use rpl_context::PatCtxt;
use rustc_hir::def_id::LocalDefId;
use rustc_hir::intravisit::{self, Visitor};
use rustc_hir::{self as hir};
use rustc_middle::hir::nested_filter::All;
use rustc_middle::ty::TyCtxt;
use rustc_span::{sym, Span};

#[instrument(level = "info", skip_all)]
pub fn check_item(tcx: TyCtxt<'_>, _pcx: PatCtxt<'_>, item_id: hir::ItemId) {
    let item = tcx.hir().item(item_id);
    let mut check_ctxt = CheckAttrCtxt { tcx };
    check_ctxt.visit_item(item);
}

struct CheckAttrCtxt<'tcx> {
    tcx: TyCtxt<'tcx>,
}

impl CheckAttrCtxt<'_> {
    fn check_inline(&self, def_id: LocalDefId) -> bool {
        let hir_id = self.tcx.local_def_id_to_hir_id(def_id);
        let attrs = self.tcx.hir().attrs(hir_id);
        for attr in attrs {
            if let [sym::inline, ..] = attr.path().as_slice() {
                let is_inline = true;
                let is_never = attr.meta_item_list().is_some_and(|meta_item_list| {
                    meta_item_list
                        .iter()
                        .any(|meta_item| meta_item.ident().is_some_and(|ident| ident.name == sym::never))
                });
                if is_inline && !is_never {
                    return true;
                }
            }
        }
        false
    }
}

impl<'tcx> Visitor<'tcx> for CheckAttrCtxt<'tcx> {
    type NestedFilter = All;
    fn nested_visit_map(&mut self) -> Self::Map {
        self.tcx.hir()
    }

    #[instrument(level = "debug", skip_all, fields(?item.owner_id))]
    fn visit_item(&mut self, item: &'tcx hir::Item<'tcx>) -> Self::Result {
        match item.kind {
            hir::ItemKind::Trait(hir::IsAuto::No, ..) | hir::ItemKind::Impl(_) | hir::ItemKind::Fn { .. } => {},
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
        if !self.tcx.visibility(def_id).is_public() && self.check_inline(def_id) {
            self.tcx.emit_node_span_lint(
                crate::lints::PRIVATE_AND_INLINE,
                self.tcx.local_def_id_to_hir_id(def_id),
                _span,
                crate::errors::PrivateFunctionMarkedInline { span: _span },
            );
        }
        intravisit::walk_fn(self, kind, decl, body_id, def_id);
    }
}
