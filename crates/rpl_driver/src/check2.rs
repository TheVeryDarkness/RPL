use std::cell::RefCell;
use std::convert::identity;
use std::ops::DerefMut;

use rpl_constraints::predicates::BodyInfoCache;
use rpl_context::PatCtxt;
use rpl_match::graph::{MirControlFlowGraph, MirDataDepGraph};
use rpl_match::mir::pat;
use rpl_match::predicate_evaluator::PredicateEvaluator;
use rpl_match::{MatchComposedPattern, MirGraph, Reachability, check2, graph, match2};
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

struct CheckFnsCtxt<'pcx, 'tcx> {
    pcx: PatCtxt<'pcx>,
    tcx: TyCtxt<'tcx>,
    body_caches: RefCell<FxHashMap<DefId, BodyInfoCache>>,
}
type Cx<'a, 'tcx> = &'a [MirGraph<'tcx>];
impl<'a, 'tcx, 'pcx> MatchComposedPattern<'pcx, 'tcx, Cx<'a, 'tcx>> for CheckFnsCtxt<'pcx, 'tcx> {
    type Matched = match2::Matched<'tcx>;
    type NormalizedMatched = match2::NormalizedMatched<'tcx>;

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
        fn_pat: &'a pat::FnPattern<'pcx>,
        cx: Cx<'a, 'tcx>,
    ) -> Vec<match2::Matched<'tcx>> {
        let pat_cfg = graph::pat_control_flow_graph(fn_pat.expect_body(), tcx.pointer_size().bytes());
        let pat_ddg = graph::pat_data_dep_graph(fn_pat.expect_body(), &pat_cfg);
        check2(tcx, pcx, pat, pat_name, &pat_cfg, &pat_ddg, fn_pat, cx)
    }
    fn check_constraints(
        &self,
        name: Symbol,
        fn_pat: &pat::FnPattern<'tcx>,
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
                .or_insert_with(|| BodyInfoCache::new(self.tcx(), typing_env, body));
        }
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
        evaluator.evaluate_constraint(&fn_pat.constraints)
    }
}
