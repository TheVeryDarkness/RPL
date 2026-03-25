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
use std::cell::RefCell;
use std::convert::identity;
use std::ops::DerefMut;

use rpl_constraints::predicates::BodyInfoCache;
use rpl_context::PatCtxt;
use rpl_context::pat::DynamicError;
use rpl_match::graph::{self, MirControlFlowGraph, MirDataDepGraph};
use rpl_match::matches::Matched;
use rpl_match::matches::artifact::NormalizedMatched;
use rpl_match::mir::{CheckMirCtxt, pat};
use rpl_match::{MatchComposedPattern, MirGraph, Reachability, check2};
use rpl_meta::context::MetaContext;
use rustc_data_structures::fx::FxHashMap;
use rustc_hir::def_id::{DefId, LocalDefId};
use rustc_hir::intravisit::{self, Visitor};
use rustc_hir::{self as hir, FnHeader};
use rustc_lint_defs::RegisteredTools;
use rustc_macros::{Diagnostic, LintDiagnostic};
use rustc_middle::hir::nested_filter;
use rustc_middle::mir;
use rustc_middle::mir::interpret::PointerArithmetic as _;
use rustc_middle::ty::{self, TyCtxt};
use rustc_middle::util::Providers;
use rustc_session::declare_tool_lint;
use rustc_span::symbol::Ident;
use rustc_span::{Span, Symbol};

#[cfg(feature = "timing")]
mod errors;

#[cfg(feature = "timing")]
pub use errors::{TIMING, Timing};

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

    let mut check_ctxt = CheckFnCtxt {
        tcx,
        pcx,
        body_caches: RefCell::default(),
    };
    tcx.hir().walk_toplevel_module(&mut check_ctxt);

    // walk2(tcx, pcx);

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

// pub fn check_item(tcx: TyCtxt<'_>, pcx: PatCtxt<'_>, item_id: hir::ItemId) {
//     let item = tcx.hir().item(item_id);
//     // let def_id = item_id.owner_id.def_id;
//     let mut check_ctxt = CheckFnCtxt { tcx, pcx };
//     check_ctxt.visit_item(item);
// }

fn fn_name<'tcx>(kind: intravisit::FnKind<'tcx>) -> (Option<Ident>, Option<FnHeader>) {
    match kind {
        intravisit::FnKind::ItemFn(name, _, fn_header) => (Some(name), Some(fn_header)),
        intravisit::FnKind::Method(name, fn_sig) => (Some(name), Some(fn_sig.header)),
        intravisit::FnKind::Closure => (None, None),
    }
}

/// Used for finding pattern matches in given Rust crate.
struct CheckFnCtxt<'pcx, 'tcx> {
    tcx: TyCtxt<'tcx>,
    pcx: PatCtxt<'pcx>,
    body_caches: RefCell<FxHashMap<DefId, BodyInfoCache>>,
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

fn walk2<'pcx, 'tcx>(tcx: TyCtxt<'tcx>, pcx: PatCtxt<'pcx>) {
    struct CheckCtxt2<'tcx> {
        tcx: TyCtxt<'tcx>,
        graphs: Vec<MirGraph<'tcx>>,
    }
    impl<'tcx> Visitor<'tcx> for CheckCtxt2<'tcx> {
        type NestedFilter = nested_filter::All;
        fn nested_visit_map(&mut self) -> Self::Map {
            self.tcx.hir()
        }

        fn visit_fn(
            &mut self,
            fk: intravisit::FnKind<'tcx>,
            fd: &'tcx rustc_hir::FnDecl<'tcx>,
            _: rustc_hir::BodyId,
            _: Span,
            id: LocalDefId,
        ) -> Self::Result {
            trace!(?id, is_mir_available = ?self.tcx.is_mir_available(id), "visit_fn");
            if self.tcx.is_mir_available(id) {
                let body = self.tcx.optimized_mir(id);
                let self_ty = self
                    .tcx
                    .impl_of_method(id.into())
                    .map(|impl_| self.tcx.type_of(impl_).instantiate_identity());

                let has_self = fd.implicit_self.has_implicit_self();
                let typing_env = ty::TypingEnv::post_analysis(self.tcx, body.source.def_id());
                let (name, _) = fn_name(fk);

                let mir_cfg = graph::mir_control_flow_graph(body);
                let mir_ddg = graph::mir_data_dep_graph(body, &mir_cfg);
                self.graphs.push(MirGraph {
                    body,
                    self_ty,
                    has_self,
                    mir_cfg,
                    mir_ddg,
                    typing_env,
                    id,
                    decl: fd,
                    name,
                    reachability: Reachability::<mir::BasicBlock>::new_mir(body),
                });
            }
        }
    }

    let mut cx = CheckCtxt2 {
        tcx,
        graphs: Vec::new(),
    };
    tcx.hir().walk_toplevel_module(&mut cx);

    let source_map = tcx.sess.source_map();
    let graphs = cx
        .graphs
        .iter()
        .map(|graph| (graph.id, (graph.name.map(|n| n.name), graph.body, graph.decl)))
        .collect::<FxHashMap<_, _>>();
    pcx.for_each_rpl_pattern(|_id, pattern| {
        for (&name, pat_item) in &pattern.patt_block {
            match pat_item {
                pat::PatternItem::RustItems(items) => {
                    for fn_pat in &items.fns {
                        let mir_pat = fn_pat.body.unwrap();
                        let pat_cfg = graph::pat_control_flow_graph(mir_pat, tcx.pointer_size().bytes());
                        let pat_ddg = graph::pat_data_dep_graph(mir_pat, &pat_cfg);
                        let matched = check2(tcx, pcx, items, name, &pat_cfg, &pat_ddg, fn_pat, &cx.graphs);
                        for matched in matched {
                            let bottom = matched.bottom();
                            // let matched = NormalizedMatched::new(&matched, label_map, &attr_map);
                            let error = pattern
                                .get_diag(name, source_map, &graphs, &matched)
                                .unwrap_or_else(identity);
                            tcx.emit_node_span_lint(
                                error.lint(),
                                tcx.local_def_id_to_hir_id(bottom),
                                error.primary_span().clone(),
                                error,
                            );
                        }
                    }
                },
                pat::PatternItem::RPLPatternOperation(_) => {
                    todo!();
                },
            }
        }
    });
}
