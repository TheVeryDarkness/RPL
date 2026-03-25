use std::cell::RefCell;
use std::convert::identity;
use std::ops::DerefMut;

use rpl_constraints::predicates::BodyInfoCache;
use rpl_context::PatCtxt;
use rpl_context::pat::DynamicError;
use rpl_match::MatchComposedPattern;
use rpl_match::graph::{self, MirControlFlowGraph, MirDataDepGraph};
use rpl_match::matches::Matched;
use rpl_match::matches::artifact::NormalizedMatched;
use rpl_match::mir::{CheckMirCtxt, pat};
use rustc_data_structures::fx::FxHashMap;
use rustc_hir::def_id::{DefId, LocalDefId};
use rustc_hir::intravisit::{self, Visitor};
use rustc_hir::{self as hir, FnHeader};
use rustc_middle::hir::nested_filter;
use rustc_middle::mir;
use rustc_middle::ty::{self, TyCtxt};
use rustc_span::symbol::Ident;
use rustc_span::{Span, Symbol};

use crate::utils::fn_name;

// pub fn check_item(tcx: TyCtxt<'_>, pcx: PatCtxt<'_>, item_id: hir::ItemId) {
//     let item = tcx.hir().item(item_id);
//     // let def_id = item_id.owner_id.def_id;
//     let mut check_ctxt = CheckFnCtxt { tcx, pcx };
//     check_ctxt.visit_item(item);
// }

/// Used for finding pattern matches in given Rust crate.
pub(crate) struct CheckFnCtxt<'pcx, 'tcx> {
    tcx: TyCtxt<'tcx>,
    pcx: PatCtxt<'pcx>,
    body_caches: RefCell<FxHashMap<DefId, BodyInfoCache>>,
}

impl<'pcx, 'tcx> CheckFnCtxt<'pcx, 'tcx> {
    pub(crate) fn new(tcx: TyCtxt<'tcx>, pcx: PatCtxt<'pcx>) -> Self {
        Self {
            tcx,
            pcx,
            body_caches: RefCell::default(),
        }
    }
}

impl<'tcx> Visitor<'tcx> for CheckFnCtxt<'_, 'tcx> {
    type NestedFilter = nested_filter::All;
    fn nested_visit_map(&mut self) -> Self::Map {
        self.tcx.hir()
    }

    #[instrument(level = "debug", skip_all, fields(item_id = ?item.owner_id.def_id))]
    fn visit_item(&mut self, item: &'tcx hir::Item<'tcx>) -> Self::Result {
        match item.kind {
            // hir::ItemKind::Trait(hir::IsAuto::No, hir::Safety::Safe, ..) | hir::ItemKind::Fn { .. } => {},
            hir::ItemKind::Trait(_, _, _, _, impl_) => {
                for trait_item in impl_ {
                    self.check_trait_item_ref(trait_item, None);
                }
            },
            hir::ItemKind::Impl(impl_) => self.check_impl(
                impl_,
                Some(self.tcx.type_of(item.owner_id.def_id).instantiate_identity()),
            ),
            // hir::ItemKind::Fn { sig, .. } => self.check_fn(
            //     Some(item.ident),
            //     &sig.decl,
            //     Some(sig.header),
            //     sig.decl.implicit_self.has_implicit_self(),
            //     item.owner_id.def_id,
            // ),
            // hir::ItemKind::Struct(struct_, generics) => self.check_struct(item.owner_id.def_id, struct_, generics),
            // hir::ItemKind::Enum(enum_, generics) => self.check_enum(item.owner_id.def_id, enum_, generics),
            _ => {},
        }
        intravisit::walk_item(self, item);
    }

    #[instrument(level = "debug", skip(self, kind, decl, body_id, _span))]
    fn visit_fn(
        &mut self,
        kind: intravisit::FnKind<'tcx>,
        decl: &'tcx hir::FnDecl<'tcx>,
        body_id: hir::BodyId,
        _span: Span,
        def_id: LocalDefId,
    ) -> Self::Result {
        let (name, header) = fn_name(kind);

        let self_ty = self
            .tcx
            .impl_of_method(def_id.into())
            .map(|impl_| self.tcx.type_of(impl_).instantiate_identity());

        self.check_fn(
            name,
            decl,
            header,
            decl.implicit_self.has_implicit_self(),
            self_ty,
            def_id,
        );

        let attrs: Vec<_> = self
            .tcx
            .get_attrs_by_path(def_id.to_def_id(), &[Symbol::intern("rpl"), Symbol::intern("dynamic")])
            .collect();
        for attr in &attrs {
            let error = DynamicError::from_attr(attr, self.tcx.def_span(def_id.to_def_id()));
            self.tcx.emit_node_span_lint(
                error.lint(),
                self.tcx.local_def_id_to_hir_id(def_id),
                error.primary_span().clone(),
                error,
            );
        }

        intravisit::walk_fn(self, kind, decl, body_id, def_id);
    }
}

impl<'tcx, 'pcx> MatchComposedPattern<'pcx, 'tcx> for CheckFnCtxt<'pcx, 'tcx> {
    type Matched = Matched<'tcx>;
    type NormalizedMatched = NormalizedMatched<'tcx>;
    fn pcx(&self) -> PatCtxt<'pcx> {
        self.pcx
    }
    fn tcx(&self) -> TyCtxt<'tcx> {
        self.tcx
    }
    fn body_caches(&self) -> impl DerefMut<Target = FxHashMap<DefId, BodyInfoCache>> {
        self.body_caches.borrow_mut()
    }
    fn check_mir<'a>(
        tcx: TyCtxt<'tcx>,
        pcx: PatCtxt<'pcx>,
        body: &'a mir::Body<'tcx>,
        has_self: bool,
        self_ty: Option<ty::Ty<'tcx>>,
        pat: &'pcx pat::RustItems<'pcx>,
        pat_name: Symbol,
        fn_pat: &'a pat::FnPattern<'pcx>,
        mir_cfg: &'a MirControlFlowGraph,
        mir_ddg: &'a MirDataDepGraph,
    ) -> Vec<Matched<'tcx>> {
        CheckMirCtxt::new(
            tcx, pcx, body, has_self, self_ty, pat, pat_name, fn_pat, mir_cfg, mir_ddg,
        )
        .check()
    }
}

impl<'tcx> CheckFnCtxt<'_, 'tcx> {
    #[instrument(level = "debug", skip(self, trait_item), fields(trait_item_id = ?trait_item.id))]
    fn check_trait_item_ref(&mut self, trait_item: &'tcx hir::TraitItemRef, self_ty: Option<ty::Ty<'tcx>>) {
        if let hir::AssocItemKind::Fn { has_self } = trait_item.kind {
            let id = trait_item.id;
            let trait_item = self.tcx.hir().trait_item(id);
            let def_id = trait_item.owner_id.def_id;
            match trait_item.kind {
                hir::TraitItemKind::Fn(sig, _) => {
                    self.check_assoc_fn(has_self, self_ty, &sig, def_id);
                },
                _ => (), // Actually impossible, but we handle it gracefully.
            }
        }
    }
    #[instrument(level = "debug", skip(self, impl_))]
    fn check_impl(&mut self, impl_: &hir::Impl<'tcx>, self_ty: Option<ty::Ty<'tcx>>) {
        for impl_item in impl_.items {
            self.check_impl_item_ref(impl_item, self_ty);
        }
    }
    #[instrument(level = "debug", skip(self, impl_item), fields(impl_item_id = ?impl_item.id))]
    fn check_impl_item_ref(&mut self, impl_item: &'tcx hir::ImplItemRef, self_ty: Option<ty::Ty<'tcx>>) {
        if let hir::AssocItemKind::Fn { has_self } = impl_item.kind {
            let id = impl_item.id;
            let impl_item = self.tcx.hir().impl_item(id);
            let def_id = impl_item.owner_id.def_id;
            match impl_item.kind {
                hir::ImplItemKind::Fn(sig, _) => {
                    self.check_assoc_fn(has_self, self_ty, &sig, def_id);
                },
                _ => (), // Actually impossible, but we handle it gracefully.
            }
        }
    }
    #[instrument(level = "debug", skip(self, sig), fields(is_mir_available = ?self.tcx.is_mir_available(def_id)))]
    fn check_assoc_fn(
        &mut self,
        has_self: bool,
        self_ty: Option<ty::Ty<'tcx>>,
        sig: &hir::FnSig<'tcx>,
        def_id: LocalDefId,
    ) {
        if self.tcx.is_mir_available(def_id) {
            let decl = sig.decl;
            let body = self.tcx.optimized_mir(def_id);
            let mir_cfg = graph::mir_control_flow_graph(body);
            let mir_ddg = graph::mir_data_dep_graph(body, &mir_cfg);
            let header = Some(sig.header);
            let source_map = self.tcx.sess.source_map();
            self.pcx.for_each_rpl_pattern(|_id, pattern| {
                for (&name, pat_item) in &pattern.patt_block {
                    for matched in self.impl_matched_pat_item(
                        name, pat_item, def_id, header, has_self, self_ty, body, &mir_cfg, &mir_ddg,
                    ) {
                        let error = pattern
                            .get_diag(name, source_map, (body, decl, None), &matched)
                            .unwrap_or_else(identity);
                        self.tcx.emit_node_span_lint(
                            error.lint(),
                            self.tcx.local_def_id_to_hir_id(def_id),
                            error.primary_span().clone(),
                            error,
                        );
                    }
                }
            });
        }
    }
    #[instrument(level = "debug", skip(self, decl, header))]
    fn check_fn(
        &mut self,
        fn_name: Option<Ident>,
        decl: &hir::FnDecl<'tcx>,
        header: Option<FnHeader>,
        has_self: bool,
        self_ty: Option<ty::Ty<'tcx>>,
        def_id: LocalDefId,
    ) {
        trace!(is_mir_available = ?self.tcx.is_mir_available(def_id), "check_fn");
        if self.tcx.is_mir_available(def_id) {
            let body = self.tcx.optimized_mir(def_id);
            let mir_cfg = graph::mir_control_flow_graph(body);
            let mir_ddg = graph::mir_data_dep_graph(body, &mir_cfg);
            let fn_name = fn_name.map(|ident| ident.name);
            let source_map = self.tcx.sess.source_map();
            self.pcx.for_each_rpl_pattern(|_id, pattern| {
                for (&name, pat_item) in &pattern.patt_block {
                    for matched in self.fn_matched_pat_item(
                        name, pat_item, def_id, header, has_self, self_ty, body, &mir_cfg, &mir_ddg,
                    ) {
                        let error = pattern
                            .get_diag(name, source_map, (body, decl, fn_name), &matched)
                            .unwrap_or_else(identity);
                        self.tcx.emit_node_span_lint(
                            error.lint(),
                            self.tcx.local_def_id_to_hir_id(def_id),
                            error.primary_span().clone(),
                            error,
                        );
                    }
                }
            });
        }
    }
    // #[instrument(level = "debug", skip(self))]
    // fn check_struct(
    //     &mut self,
    //     def_id: LocalDefId,
    //     variant: hir::VariantData<'tcx>,
    //     generics: &'tcx hir::Generics<'tcx>,
    // ) {
    //     let adt_def = self.tcx.adt_def(def_id);
    //     self.pcx.for_each_rpl_pattern(|_id, pattern| {
    //         for (&name, pat_item) in &pattern.patt_block {
    //             match pat_item {
    //                 rpl_context::pat::PatternItem::RustItems(rpl_rust_items) => {
    //                     for (_, adt_pat) in &rpl_rust_items.adts {
    //                         for matched in
    //                             MatchAdtCtxt::new(self.tcx, self.pcx, rpl_rust_items,
    // adt_pat).match_adt(adt_def)                         {
    //                             let error = pattern
    //                                 .get_diag(name, &fn_pat.expect_mir_body().labels, body, &matched)
    //                                 .unwrap_or_else(identity);
    //                             self.tcx.emit_node_span_lint(
    //                                 error.lint(),
    //                                 self.tcx.local_def_id_to_hir_id(def_id),
    //                                 error.primary_span(),
    //                                 error,
    //                             );
    //                         }
    //                     }
    //                 },
    //                 _ => unreachable!(),
    //             }
    //         }
    //     });
    // }
    // #[instrument(level = "debug", skip(self))]
    // fn check_enum(&mut self, def_id: LocalDefId, variants: hir::EnumDef<'tcx>, generics: &'tcx
    // hir::Generics<'tcx>) {}
}
