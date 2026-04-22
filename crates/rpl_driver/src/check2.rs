use std::cell::RefCell;
use std::convert::identity;
use std::ops::DerefMut;

use rpl_constraints::predicates::BodyInfoCache;
use rpl_context::PatCtxt;
use rpl_match::graph::{MirControlFlowGraph, MirDataDepGraph};
use rpl_match::match2::{AllMirGraphs, NormalizedMatched};
use rpl_match::mir::pat;
use rpl_match::predicate_evaluator::PredicateEvaluator;
use rpl_match::{MatchComposedPattern, MirGraph, NormalizedMatched as _, Reachability, check2, graph, match2};
use rustc_data_structures::fx::FxHashMap;
use rustc_hir::def_id::{DefId, LocalDefId};
use rustc_hir::intravisit::{self, Visitor};
use rustc_middle::hir::nested_filter;
use rustc_middle::mir;
use rustc_middle::mir::interpret::PointerArithmetic;
use rustc_middle::ty::{self, TyCtxt};
use rustc_span::{Span, Symbol};

use crate::utils::fn_name;

pub(crate) fn walk2<'pcx, 'tcx>(tcx: TyCtxt<'tcx>, pcx: PatCtxt<'pcx>) {
    let mut cx = Collector {
        tcx,
        graphs: Vec::new(),
    };
    tcx.hir().walk_toplevel_module(&mut cx);

    let source_map = tcx.sess.source_map();
    // let graphs = cx
    //     .graphs
    //     .iter()
    //     .map(|graph| (graph.id, (graph.name.map(|n| n.name), graph.body, graph.decl)))
    //     .collect::<FxHashMap<_, _>>();

    let graphs = AllMirGraphs::from(cx.graphs);
    let cx = CheckFnsCtxt {
        pcx,
        tcx,
        body_caches: RefCell::default(),
    };

    pcx.for_each_rpl_pattern(|_id, pattern| {
        for (&name, pat_item) in &pattern.patt_block {
            for matched in CheckFnsCtxt::check_mir(tcx, pcx, pat_item.expect_rust_items(), name, fn_pat, &graphs) {
                let def_id = matched.bottom;
                let matched = NormalizedMatched::new(def_id, &matched, label_map, &attr_map);
                let error = pattern
                    .get_diag(name, source_map, &graphs, &matched)
                    .unwrap_or_else(identity);
                tcx.emit_node_span_lint(
                    error.lint(),
                    tcx.local_def_id_to_hir_id(def_id),
                    error.primary_span().clone(),
                    error,
                );
            }

            // match pat_item {
            //     pat::PatternItem::RustItems(items) => {
            //         for fn_pat in &items.fns {
            //             let mir_pat = fn_pat.body.unwrap();
            //             let pat_cfg = graph::pat_control_flow_graph(mir_pat,
            // tcx.pointer_size().bytes());             let pat_ddg =
            // graph::pat_data_dep_graph(mir_pat, &pat_cfg);             let matched =
            // check2(tcx, pcx, items, name, &pat_cfg, &pat_ddg, fn_pat, &cx.graphs);
            //             for matched in matched {
            //                 let bottom = matched.bottom;
            //                 let label_map = &fn_pat.expect_body().labels;
            //                 let attr_map = fn_pat.extra_span(tcx, bottom).unwrap();
            //                 let matched = NormalizedMatched::new(bottom, &matched, label_map,
            // &attr_map);                 let error = pattern
            //                     .get_diag(name, source_map, &graphs, &matched)
            //                     .unwrap_or_else(identity);
            //                 tcx.emit_node_span_lint(
            //                     error.lint(),
            //                     tcx.local_def_id_to_hir_id(bottom),
            //                     error.primary_span().clone(),
            //                     error,
            //                 );
            //             }
            //         }
            //     },
            //     pat::PatternItem::RPLPatternOperation(_) => {
            //         todo!();
            //     },
            // }
        }
    });
}

struct Collector<'tcx> {
    tcx: TyCtxt<'tcx>,
    graphs: Vec<MirGraph<'tcx>>,
}
impl<'tcx> Visitor<'tcx> for Collector<'tcx> {
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

            let fh = fk.header().copied();

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
                header: fh,
                name,
                reachability: Reachability::<mir::BasicBlock>::new_mir(body),
            });
        }
    }
}

struct CheckFnsCtxt<'pcx, 'tcx> {
    pcx: PatCtxt<'pcx>,
    tcx: TyCtxt<'tcx>,
    body_caches: RefCell<FxHashMap<DefId, BodyInfoCache>>,
}
type Cx<'a, 'tcx> = &'a [MirGraph<'tcx>];
impl<'a, 'tcx, 'pcx> MatchComposedPattern<'a, 'pcx, 'tcx, Cx<'a, 'tcx>> for CheckFnsCtxt<'pcx, 'tcx> {
    type Matched = match2::Matched<'tcx>;
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
    fn check_mir(
        tcx: TyCtxt<'tcx>,
        pcx: PatCtxt<'pcx>,
        pat: &'pcx pat::RustItems<'pcx>,
        pat_name: Symbol,
        fn_pat: &pat::FnPattern<'pcx>,
        cx: Cx<'a, 'tcx>,
    ) -> Vec<match2::Matched<'tcx>> {
        let pat_cfg = graph::pat_control_flow_graph(fn_pat.expect_body(), tcx.pointer_size().bytes());
        let pat_ddg = graph::pat_data_dep_graph(fn_pat.expect_body(), &pat_cfg);
        check2(tcx, pcx, pat, pat_name, &pat_cfg, &pat_ddg, fn_pat, cx)
    }
    fn check_mir_rust_items(
        &self,
        rpl_rust_items: &'pcx pat::RustItems<'pcx>,
        name: Symbol,
        fn_pat: &pat::FnPattern<'pcx>,
        cx: Cx<'a, 'tcx>,
    ) -> Vec<Self::NormalizedMatched> {
        let iter = rpl_rust_items
            .fns
            .iter()
            // .filter(move |fn_pat| self.filter(name, fn_pat, def_id, header, cx))
            .flat_map(move |fn_pat| {
                Self::check_mir(self.tcx(), self.pcx(), rpl_rust_items, name, fn_pat, cx)
                    .into_iter()
                    .filter(move |matched| self.check_constraints(name, fn_pat, matched.bottom, matched, cx))
                    .map(move |matched| {
                        let labels = &fn_pat.expect_body().labels;
                        let attr_map = fn_pat.extra_span(self.tcx(), def_id)?;
                        (matched, labels, attr_map.clone())
                    })
            })
            .map(move |(matched, label_map, attr_map)| NormalizedMatched::new(def_id, &matched, label_map, &attr_map));

        rpl_rust_items.post_process(iter).collect()
    }
    fn check_constraints(
        &self,
        name: Symbol,
        fn_pat: &pat::FnPattern<'pcx>,
        bottom: LocalDefId,
        matched: &Self::Matched,
        cx: Cx<'a, 'tcx>,
    ) -> bool {
        let mut cache = self.body_caches();
        for cx in cx {
            let body = cx.body;
            let typing_env = ty::TypingEnv::post_analysis(self.tcx(), body.source.def_id());
            let cache = cache
                .entry(body.source.def_id())
                .or_insert_with(|| BodyInfoCache::new(self.tcx, typing_env, body));
            let typing_env = ty::TypingEnv::post_analysis(self.tcx, body.source.def_id());
            let evaluator = PredicateEvaluator::new(
                self.tcx(),
                typing_env,
                bottom,
                body,
                &fn_pat.expect_body().labels,
                matched,
                cache,
                fn_pat.symbol_table,
            );
            if !evaluator.evaluate_constraint(&fn_pat.constraints) {
                return false;
            }
        }
        true
    }
    fn filter(
        &self,
        name: Symbol,
        fn_pat: &pat::FnPattern<'pcx>,
        bottom: LocalDefId,
        header: Option<rustc_hir::FnHeader>,
        cx: Cx<'a, 'tcx>,
    ) -> bool {
        for cx in cx {
            if !fn_pat.filter(self.tcx, bottom, header, cx.body) {
                return false;
            }
        }
        true
    }
}
