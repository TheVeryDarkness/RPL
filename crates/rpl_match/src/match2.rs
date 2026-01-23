use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::hash::Hash;
use std::ops::{Deref, Index};

use matched::{Matched, MatchedBlock, StatementMatch};
use mitsein::vec1::Vec1;
use rpl_constraints::Const;
use rpl_context::{PatCtxt, pat};
use rustc_hash::{FxHashMap, FxHashSet};
use rustc_hir::FnDecl;
use rustc_hir::def_id::LocalDefId;
use rustc_index::{Idx, IndexVec};
use rustc_middle::ty::{TyCtxt, TypingEnv};
use rustc_middle::{mir, ty};
use rustc_span::{Ident, Symbol};
pub use with_call_stack::WithCallStack;

use crate::graph::{MirControlFlowGraph, MirDataDepGraph, PatControlFlowGraph, PatDataDepGraph};
use crate::match2::matched::NormalizedMatched;
use crate::statement::MatchStatement;
use crate::ty::MatchTy;
use crate::{AdtMatch, Reachability};

mod matched;
mod with_call_stack;

pub struct MirGraph<'tcx> {
    pub body: &'tcx mir::Body<'tcx>,
    pub has_self: bool,
    pub self_ty: Option<ty::Ty<'tcx>>,
    pub mir_cfg: MirControlFlowGraph,
    pub mir_ddg: MirDataDepGraph,
    pub typing_env: TypingEnv<'tcx>,
    pub id: LocalDefId,
    pub decl: &'tcx FnDecl<'tcx>,
    pub name: Option<Ident>,
    pub reachability: Reachability<mir::BasicBlock>,
}

#[derive(Clone)]
struct Matchings<'a, 'tcx> {
    graph: &'a MirGraph<'tcx>,
    /// List of functions that call this function. Used for propagating matches.
    ///
    /// Each caller is represented as (caller function id, call location in caller).
    callers: Vec<(LocalDefId, mir::Location)>,
    matches: Vec<Matching<'tcx>>,
}

impl Matchings<'_, '_> {
    // FIXME: this deduplication is inefficient, we should use a better data structure
    #[instrument(level = "trace", skip(self), fields(fn_id = ?self.graph.id))]
    fn dedup(&mut self) {
        trace!(num_matches = ?self.matches.len(), "deduplicating matches");
        let mut unique = Vec::new();
        for m in self.matches.drain(..) {
            if !unique.contains(&m) {
                unique.push(m);
            }
        }
        self.matches = unique;
        trace!(num_matches = ?self.matches.len(), "deduplicated matches");
    }
}

type AllMatchings<'a, 'tcx> = FxHashMap<LocalDefId, Matchings<'a, 'tcx>>;

#[instrument(level = "debug", skip(matchings))]
fn log_matchings(matchings: &AllMatchings<'_, '_>, name: &str) {
    for (match_fn_id, matchings) in matchings.iter() {
        debug!(?match_fn_id, num_matches = matchings.matches.len());
        trace_span!("log_matchings", ?name, ?match_fn_id, num_matches = ?matchings.matches.len()).in_scope(|| {
            for matching in matchings.matches.iter() {
                matching.log_matched();
            }
        });
    }
}

macro_rules! log_matchings {
    ($expr:ident) => {
        log_matchings(&$expr, stringify!($expr));
    };
}

/// Create a new matcher instance.
///
/// See the following constructors:
///
/// - [`CheckMirCtxt::new`]
///
/// [`CheckMirCtxt::new`]: crate::mir::CheckMirCtxt::new
struct MatchCtxt2<'a, 'pcx, 'tcx> {
    tcx: TyCtxt<'tcx>,
    pcx: PatCtxt<'pcx>,
    pat: &'pcx pat::RustItems<'pcx>,
    pat_name: Symbol,
    fn_pat: &'a pat::FnPattern<'pcx>,
    pat_cfg: &'a PatControlFlowGraph,
    pat_ddg: &'a PatDataDepGraph,
    reachability: &'a Reachability<pat::BasicBlock>,
    /// Copied from [`crate::place::MatchPlaceCtxt`].
    places: IndexVec<pat::PlaceVarIdx, pat::Ty<'pcx>>,
    fns: &'a [MirGraph<'tcx>],
}

impl<'a, 'pcx, 'tcx: 'a> MatchCtxt2<'a, 'pcx, 'tcx> {
    fn new_matching(&self, fn_pat: &pat::FnPatternBody<'pcx>, body: &mir::Body<'tcx>) -> Matching<'tcx> {
        let cx = self;
        let num_blocks = fn_pat.basic_blocks.len();
        let num_locals = fn_pat.locals.len();
        let mir_statements = IndexVec::from_fn_n(
            |bb| {
                MirStatementBackMatches::from_fn_n(
                    |_| MatchingCell::new(),
                    body[bb].statements.len() + body[bb].terminator.is_some() as usize,
                )
            },
            body.basic_blocks.len(),
        );
        Matching {
            basic_blocks: IndexVec::from_fn_n(
                |bb_pat| {
                    let num_stmt_pats = fn_pat[bb_pat].num_statements_and_terminator();
                    // We don't need to match the end of the pattern, because it is only a marker and has no
                    // corresponding terminator.
                    // if fn_pat[bb_pat].has_pat_end() {
                    //     num_stmt_pats -= 1;
                    // }
                    MatchingBlock::from_fn_n(|_| MatchingCell::new(), num_stmt_pats)
                },
                num_blocks,
            ),
            locals: IndexVec::from_fn_n(|_| MatchingCell::new(), num_locals),
            ty_vars: IndexVec::from_fn_n(|_| MatchingCell::new(), cx.fn_pat.meta.ty_vars.len()),
            const_vars: IndexVec::from_fn_n(|_| MatchingCell::new(), cx.fn_pat.meta.const_vars.len()),
            place_vars: IndexVec::from_fn_n(|_| MatchingCell::new(), cx.fn_pat.meta.place_vars.len()),
            mir_statements,
            adt_matches: FxHashMap::default(),
        }
    }
    fn new_ctx(
        &'a self,
        fn_pat: &'a pat::FnPatternBody<'pcx>,
        fn_graph: &MirGraph<'tcx>,
    ) -> MatchCtxt2Once<'a, 'pcx, 'tcx> {
        MatchCtxt2Once {
            cx: self,
            has_self: fn_graph.has_self,
            def_id: fn_graph.id,
            body: fn_graph.body,
            self_ty: fn_graph.self_ty,
            typing_env: fn_graph.typing_env,
            fn_pat,
            pat: self.pat,
            matching: self.new_matching(fn_pat, fn_graph.body),
        }
    }
    /// Find all possible matches of 1-component in pattern graph to MIR graph.
    #[instrument(level = "debug", skip_all, fields(pat_name = ?self.pat_name))]
    fn find_matches_1(&self) -> AllMatchings<'a, 'tcx> {
        let mut matching_1 = AllMatchings::default();
        if let Some(fn_pat) = self.fn_pat.body {
            for fn_graph in self.fns {
                let mut matchings = Matchings {
                    graph: fn_graph,
                    callers: Vec::new(),
                    matches: Vec::new(),
                };
                debug!(id = ?fn_graph.id, ?self.fn_pat.name, "matching function");
                // Find all possible matches of 1-component in pattern graph to MIR graph.
                for (bb_pat, block_pat) in fn_pat.basic_blocks.iter_enumerated() {
                    for (stmt_pat_idx, stmt_pat) in block_pat.statements.iter().enumerate() {
                        let loc_pat = pat::Location {
                            block: bb_pat,
                            statement_index: stmt_pat_idx,
                        };

                        // Match arguments
                        if loc_pat.statement_index < block_pat.statements.len()
                            && let pat::StatementKind::Assign(
                                pat::Place {
                                    base: pat::PlaceBase::Local(local_pat),
                                    projection: [],
                                },
                                pat::Rvalue::Any,
                            ) = block_pat.statements[loc_pat.statement_index]
                        {
                            if fn_pat.self_idx == Some(local_pat) && fn_graph.has_self {
                                let self_value = mir::Local::from_u32(1);
                                let cx = self.new_ctx(fn_pat, fn_graph);
                                if cx.match_local(local_pat, self_value) {
                                    let loc = WithCallStack::new_one(fn_graph.id, StatementMatch::Arg(self_value));
                                    cx.matching[loc_pat].set_checked(loc);
                                    matchings.matches.push(cx.matching);
                                }
                            } else {
                                for arg in fn_graph.body.args_iter() {
                                    let _span = debug_span!(
                                        "build_candidates",
                                        arg = ?StatementMatch::Arg(arg).debug_with(fn_graph.body)
                                    )
                                    .entered();
                                    let cx = self.new_ctx(fn_pat, fn_graph);
                                    if cx.match_local(local_pat, arg) {
                                        info!(
                                            "candidate matched: {loc_pat:?} {pat:?} <-> {arg:?}",
                                            pat = cx.mir_pat()[bb_pat].debug_stmt_at(stmt_pat_idx),
                                        );
                                        let loc = WithCallStack::new_one(fn_graph.id, StatementMatch::Arg(arg));
                                        cx.matching[loc_pat].set_checked(loc);
                                        matchings.matches.push(cx.matching);
                                    }
                                }
                            }
                        }

                        // Match statements
                        for (bb, block) in fn_graph.body.basic_blocks.iter_enumerated() {
                            for stmt_idx in 0..block.statements.len() + block.terminator.is_some() as usize {
                                let loc = mir::Location {
                                    block: bb,
                                    statement_index: stmt_idx,
                                };
                                let cx = self.new_ctx(fn_pat, fn_graph);
                                if cx.match_statement_or_terminator(loc_pat, loc) {
                                    let loc_with_stack =
                                        WithCallStack::new_one(fn_graph.id, StatementMatch::Location(loc));
                                    cx.matching[loc_pat].set_checked(loc_with_stack);
                                    cx.matching[loc].set_checked(Vec1::from_one(loc_pat));
                                    matchings.matches.push(cx.matching);
                                    // trace!(?fn_graph.id, ?loc_pat, ?loc, "found 1-component
                                    // match");
                                }
                            }
                        }
                    }
                    if let Some(terminator_pat) = &block_pat.terminator {
                        let loc_pat = pat::Location {
                            block: bb_pat,
                            statement_index: block_pat.num_statements(),
                        };
                        for (bb, block) in fn_graph.body.basic_blocks.iter_enumerated() {
                            if let Some(terminator) = &block.terminator {
                                let loc = mir::Location {
                                    block: bb,
                                    statement_index: block.statements.len(),
                                };
                                let cx = self.new_ctx(fn_pat, fn_graph);
                                if cx.match_terminator(loc_pat, loc, &terminator_pat, &terminator) {
                                    let loc_with_stack =
                                        WithCallStack::new_one(fn_graph.id, StatementMatch::Location(loc));
                                    cx.matching[loc_pat].set_checked(loc_with_stack);
                                    cx.matching[loc].set_checked(Vec1::from_one(loc_pat));
                                    matchings.matches.push(cx.matching);
                                    // trace!(?fn_graph.id, ?loc_pat, ?loc, "found 1-component
                                    // match");
                                }
                            }
                        }
                    }
                }
                debug!(?fn_graph.id, num = ?matchings.matches.len(), "found 1-component match");

                matching_1.insert(fn_graph.id, matchings);
            }
        }

        for fn_graph in self.fns {
            for (bb, block) in fn_graph.body.basic_blocks.iter_enumerated() {
                if let Some(mir::Terminator {
                    kind: mir::TerminatorKind::Call { func, .. },
                    ..
                }) = &block.terminator
                    && let mir::Operand::Constant(box mir::ConstOperand { const_, .. }) = func
                    && let ty::FnDef(callee_id, ..) = *const_.ty().kind()
                    && let Some(callee_id) = callee_id.as_local()
                    && let Some(m) = matching_1.get_mut(&callee_id)
                {
                    let loc = mir::Location {
                        block: bb,
                        statement_index: block.statements.len(),
                    };
                    m.callers.push((fn_graph.id, loc));
                    trace!(caller = ?fn_graph.id, callee = ?callee_id, ?loc, "found caller");
                }
            }
        }

        if cfg!(debug_assertions) {
            for matching in matching_1.values() {
                for (caller_id, caller_loc) in &matching.callers {
                    Matching::check(*caller_loc, *caller_id, &matching_1);
                }
            }
        }

        matching_1
    }

    /// Join matches of `k` components with matches of `1` components to form matches of `k+1`
    /// components.
    #[instrument(
        level = "debug",
        skip_all,
        fields(
            num_fn_matches_k = ?matches_k.len(),
            num_fn_matches_1 = ?matches_1.len(),
        )
    )]
    fn join_matches(
        &self,
        matches_k: &AllMatchings<'a, 'tcx>,
        matches_1: &AllMatchings<'a, 'tcx>,
    ) -> AllMatchings<'a, 'tcx> {
        // log_matchings(matches_k, "matches_k");
        // log_matchings(matches_1, "matches_1");
        let mut matches_k1 = AllMatchings::default();
        for (fn_id, matchings_k) in matches_k.iter() {
            if let Some(matchings_1) = matches_1.get(fn_id) {
                let mut matchings_k1 = Matchings {
                    graph: matchings_k.graph,
                    callers: matchings_k.callers.clone(),
                    matches: Vec::new(),
                };
                for matching_k in matchings_k.matches.iter() {
                    if matching_k.is_complete() {
                        matchings_k1.matches.push(matching_k.clone());
                        continue;
                    }
                    for matching_1 in matchings_1.matches.iter() {
                        // Try to join `matching_k` and `matching_1` into `matching_k1`.
                        // If any conflict happens, discard this join.
                        if let Some(matching_k1) =
                            matching_k.join(matching_1, &matchings_k.graph.reachability, &self.reachability)
                        {
                            matchings_k1.matches.push(matching_k1);
                        }
                    }
                }

                matchings_k1.dedup();
                // if !matchings_k1.matches.is_empty() {
                //     trace!(?fn_id, num_matches_k1 = ?matchings_k1.matches.len(), "joined matches
                // for function");     // matchings_k1.matches.dedup();
                //     let matchings = take(&mut matchings_k1.matches);

                //     // FIXME: this deduplication is inefficient, we should use a better data
                // structure     for matching_k1 in matchings {
                //         if !matchings_k1.matches.contains(&matching_k1) {
                //             matchings_k1.matches.push(matching_k1);
                //         }
                //     }
                // }
                // trace!(?fn_id, num_matches_k1 = ?matchings_k1.matches.len(), "deduped joined
                // matches for function");
                matches_k1.insert(*fn_id, matchings_k1);
            }
        }
        debug!(num_matches_k1 = ?matches_k1.len(), "joined matches");
        log_matchings!(matches_k1);
        matches_k1
    }

    /// Propagate matches along call graph, from callees to callers.
    #[instrument(level = "debug", skip_all, fields(num_fn_matches = ?matchings.len()))]
    fn propagate(&self, matchings: &mut AllMatchings<'a, 'tcx>) {
        for (fn_id, matchings_fn) in matchings.iter() {
            debug!(?fn_id, num_matches = ?matchings_fn.matches.len(), "before propagation");
        }
        let mut to_visit: VecDeque<_> = matchings.keys().cloned().collect();

        while let Some(fn_id) = to_visit.pop_front() {
            // FIXME: this clone is sometimes unnecessary
            if let Some(matchings_fn) = matchings.get(&fn_id).cloned() {
                for matching in matchings_fn.matches.iter() {
                    for (caller_id, caller_loc) in &matchings_fn.callers {
                        if cfg!(debug_assertions) {
                            Matching::check(*caller_loc, *caller_id, matchings);
                        }
                        let caller_body = matchings.get(caller_id).unwrap().graph.body;
                        let m = matchings.get_mut(caller_id).unwrap();
                        let propagated = matching.propagate(*caller_loc, caller_body, *caller_id);
                        if !m.matches.contains(&propagated) {
                            m.matches.push(propagated);
                            if !to_visit.contains(caller_id) {
                                to_visit.push_back(*caller_id);
                            }
                        }
                    }
                }
            }
        }

        // for (fn_id, matchings_fn) in matchings.iter_mut() {
        //     for (caller_id, caller_loc) in &matchings_fn.callers {
        //         if let Some(caller_matchings) = matchings.get_mut(caller_id) {
        //             let mut new_matches = Vec::new();
        //             for caller_matching in &caller_matchings.matches {
        //                 for callee_matching in &matchings_fn.matches {
        //                     let new_matching = caller_matching.propagate(*caller_loc);
        //                     new_matches.push(new_matching);
        //                 }
        //             }
        //             caller_matchings.matches.extend(new_matches);
        //         }
        //     }
        // }
        for (fn_id, matchings_fn) in matchings.iter() {
            debug!(?fn_id, num_matches = ?matchings_fn.matches.len(), "after propagation");
        }
        log_matchings!(matchings);
    }
}

struct MatchCtxt2Once<'a, 'pcx, 'tcx> {
    cx: &'a MatchCtxt2<'a, 'pcx, 'tcx>,
    has_self: bool,
    def_id: LocalDefId,
    body: &'a mir::Body<'tcx>,
    self_ty: Option<ty::Ty<'tcx>>,
    typing_env: TypingEnv<'tcx>,
    fn_pat: &'a pat::FnPatternBody<'pcx>,
    pat: &'pcx pat::RustItems<'pcx>,
    matching: Matching<'tcx>,
}

impl<'a, 'pcx, 'tcx> MatchStatement<'pcx, 'tcx> for MatchCtxt2Once<'a, 'pcx, 'tcx> {
    fn has_self(&self) -> bool {
        self.has_self
    }
    fn body(&self) -> &mir::Body<'tcx> {
        self.body
    }

    fn fn_pat(&self) -> &pat::FnPattern<'pcx> {
        self.cx.fn_pat
    }
    fn mir_pat(&self) -> &pat::FnPatternBody<'pcx> {
        self.fn_pat
    }

    fn pat_cfg(&self) -> &PatControlFlowGraph {
        &self.cx.pat_cfg
    }
    fn pat_ddg(&self) -> &PatDataDepGraph {
        &self.cx.pat_ddg
    }
    fn mir_cfg(&self) -> &MirControlFlowGraph {
        &self.cx.fns[0].mir_cfg
    }
    fn mir_ddg(&self) -> &MirDataDepGraph {
        &self.cx.fns[0].mir_ddg
    }

    fn pat(&self) -> &'pcx pat::RustItems<'pcx> {
        self.pat
    }

    fn pcx(&self) -> PatCtxt<'pcx> {
        self.cx.pcx
    }

    fn tcx(&self) -> TyCtxt<'tcx> {
        self.cx.tcx
    }

    fn typing_env(&self) -> TypingEnv<'tcx> {
        self.typing_env
    }

    type MatchTy = Self;
    fn ty(&self) -> &Self::MatchTy {
        self
    }

    #[instrument(level = "trace", skip(self), ret)]
    fn match_local(&self, pat: pat::Local, local: mir::Local) -> bool {
        self.ty()
            .match_ty(self.mir_pat().locals[pat], self.body().local_decls[local].ty)
            && self.matching[pat].try_set(WithCallStack::new_one(self.def_id, local))
    }
    #[instrument(level = "trace", skip(self), ret)]
    fn match_place_var(&self, pat: pat::PlaceVarIdx, place: mir::PlaceRef<'tcx>) -> bool {
        let place_ty = place.ty(&self.body.local_decls, self.ty().tcx);
        let matched = self.ty().match_ty(self.places[pat], place_ty.ty);
        matched && self.matching[pat].try_set(WithCallStack::new_one(self.def_id, place))
    }
    fn get_place_ty_from_place_var(&self, var: pat::PlaceVarIdx) -> pat::PlaceTy<'pcx> {
        pat::PlaceTy::from_ty(self.places[var])
    }
}

impl<'a, 'pcx, 'tcx> MatchTy<'pcx, 'tcx> for MatchCtxt2Once<'a, 'pcx, 'tcx> {
    fn self_ty(&self) -> Option<ty::Ty<'tcx>> {
        self.self_ty
    }
    fn pat(&self) -> &'pcx pat::RustItems<'pcx> {
        self.pat
    }
    fn pcx(&self) -> PatCtxt<'pcx> {
        self.pcx
    }
    fn tcx(&self) -> TyCtxt<'tcx> {
        self.tcx
    }
    fn typing_env(&self) -> TypingEnv<'tcx> {
        self.typing_env
    }

    #[instrument(level = "trace", skip(self), ret)]
    fn match_ty_var(&self, ty_var: pat::TyVar, ty: ty::Ty<'tcx>) -> bool {
        self.matching[ty_var.idx].try_set(ty)
    }
    #[instrument(level = "trace", skip(self), ret)]
    fn match_ty_const_var(&self, const_var: pat::ConstVar<'pcx>, konst: ty::Const<'tcx>) -> bool {
        match konst.kind() {
            ty::ConstKind::Param(param) => {
                let ty = param.find_ty_from_env(self.typing_env.param_env);
                self.match_ty(const_var.ty, ty) && self.matching[const_var.idx].try_set(param.into())
            },
            ty::ConstKind::Value(value) => {
                self.match_ty(const_var.ty, value.ty) && {
                    let const_value = self.tcx.valtree_to_const_val(value);
                    self.matching[const_var.idx].try_set(Const::MIR(mir::Const::from_value(const_value, value.ty)))
                }
            },
            _ => false,
        }
    }
    #[instrument(level = "trace", skip(self), ret)]
    fn match_mir_const_var(&self, const_var: pat::ConstVar<'pcx>, konst: mir::Const<'tcx>) -> bool {
        self.match_ty(const_var.ty, konst.ty()) && self.matching[const_var.idx].try_set(konst.into())
    }
    #[instrument(level = "trace", skip(self), ret)]
    fn match_adt_matches(&self, pat: Symbol, adt_match: AdtMatch<'tcx>) -> bool {
        self.matching[pat].try_set(adt_match)
    }
    fn adt_matched(&self, adt_pat: Symbol, adt: ty::AdtDef<'tcx>, f: impl FnOnce(&AdtMatch<'tcx>)) {
        let guard = self.matching[adt_pat].borrow();
        guard
            .as_ref()
            .map(|matched| (matched.adt == adt).then(|| f(&matched)))
            .flatten()
            .unwrap_or_default()
    }
}

// impl<'a, 'pcx, 'tcx> MatchCtxt2Once<'a, 'pcx, 'tcx> {
//     #[must_use]
//     fn match_place(
//         &self,
//         place_pat: &pat::Place<'pcx>,
//         place: mir::PlaceRef<'tcx>,
//         matching: &mut Matching<'tcx>,
//     ) -> Option<()> {
//         match place_pat.base {
//             pat::PlaceBase::Local(pat_local) => {
//                 matching[pat_local].set(place.local)?;
//                 if place_pat.projection.len() != place.projection.len() {
//                     return None;
//                 }
//                 if !zip(
//                     iter_place_pat_proj_and_ty(self.pat, *place_pat,
// self.get_place_ty_from_base(place_pat.base)),
// iter_place_proj_and_ty(self.body, self.tcx, place),                 )
//                 .inspect(|((proj_pat, place_pat_ty), (proj, place_ty))| {
//                     trace!(?place_pat_ty, ?proj_pat, ?place_ty, ?proj, "match_place")
//                 })
//                 .all(|pair| self.match_place_elem(pair))
//                 {
//                     return None;
//                 }
//             },
//             pat::PlaceBase::Var(pat_var) => {
//                 let place_pat_proj_and_ty: Vec<_> =
//                     iter_place_pat_proj_and_ty(self.pat, *place_pat,
// self.get_place_ty_from_base(place_pat.base))                         .collect();
//                 let mut place_mir_proj_and_ty: Vec<_> = iter_place_proj_and_ty(self.body,
// self.tcx, place).collect();                 let mut place_stripping = place;
//                 for (place_pat_proj, place_pat_ty) in place_pat_proj_and_ty.into_iter().rev() {
//                     if let Some((place_proj, place_ty)) = place_mir_proj_and_ty.pop() {
//                         if !self.match_place_elem(((place_pat_proj, place_pat_ty), (place_proj,
// place_ty))) {                             return None;
//                         }
//                         place_stripping.projection =
// place_stripping.projection.split_last().unwrap().1;                     } else {
//                         return None;
//                     }
//                 }
//                 matching[pat_var].set(place_stripping)?;
//             },
//             pat::PlaceBase::Any => (),
//         };
//         Some(())
//     }
//     #[must_use]
//     fn match_rvalue(
//         &self,
//         rvalue_pat: &pat::Rvalue<'pcx>,
//         rvalue: &mir::Rvalue<'tcx>,
//         matching: &mut Matching<'tcx>,
//     ) -> Option<()> {
//         todo!()
//     }
//     #[must_use]
//     fn match_intrinsic(
//         &self,
//         intrinsic_pat: &pat::NonDivergingIntrinsic<'pcx>,
//         intrinsic: &mir::NonDivergingIntrinsic<'tcx>,
//         matching: &mut Matching<'tcx>,
//     ) -> Option<()> {
//         todo!()
//     }
//     #[must_use]
//     fn match_stmt(
//         &self,
//         loc_pat: pat::Location,
//         stmt_pat: &pat::StatementKind<'pcx>,
//         loc: mir::Location,
//         stmt: &mir::Statement<'tcx>,
//         matching: &mut Matching<'tcx>,
//     ) -> Option<()> {
//         match (stmt_pat, &stmt.kind) {
//             (pat::StatementKind::Assign(pat_place, pat_rvalue), mir::StatementKind::Assign(box
// (place, rvalue))) => {                 self.match_place(pat_place, place.as_ref(), matching)?;
//                 self.match_rvalue(pat_rvalue, rvalue, matching)?;
//             },
//             (pat::StatementKind::Intrinsic(pat_intrinsic), mir::StatementKind::Intrinsic(box
// intrinsic)) => {                 self.match_intrinsic(pat_intrinsic, intrinsic, matching)?;
//             },
//             (_, _) => None?,
//         }
//         matching[loc_pat].set(loc)?;
//         matching[loc].set(loc_pat)?;
//         Some(())
//     }
// }

impl<'a, 'pcx, 'tcx> Deref for MatchCtxt2Once<'a, 'pcx, 'tcx> {
    type Target = MatchCtxt2<'a, 'pcx, 'tcx>;

    fn deref(&self) -> &Self::Target {
        self.cx
    }
}

trait MatchingCell<T> {
    /// Set the matching to `new`.
    ///
    /// # Returns
    ///
    /// Returns `true` if `self` was `None` or equal to `new`, `false` otherwise.
    ///
    /// If and only if it returns `true`, `self` is set to `Some(new)`, otherwise `self` is
    /// unchanged.
    #[must_use]
    fn try_set(&self, new: T) -> bool;
    fn new() -> Self;

    fn set_checked(&self, new: T) {
        let ok = self.try_set(new);
        debug_assert!(ok, "try_set failed in set_checked");
    }
}

impl<T: PartialEq> MatchingCell<T> for Cell<Option<T>> {
    #[must_use]
    fn try_set(&self, new: T) -> bool {
        match self.take() {
            None => {
                self.set(Some(new));
                true
            },
            Some(old) if old == new => {
                self.set(Some(new));
                true
            },
            Some(old) => {
                self.set(Some(old));
                false
            },
        }
        // if self.get().as_ref().is_none_or(|old| *old == new) {
        //     self.set(Some(new));
        //     true
        // } else {
        //     false
        // }
    }
    fn new() -> Self {
        Cell::new(None)
    }
}

// impl<T: PartialEq> MatchingCell<Vec<T>> for VecCell<T> {
//     #[must_use]
//     fn try_set(&self, new: Vec<T>) -> bool {
//         let old = self.take();
//         if old.is_empty() || old == new {
//             self.set(new);
//             true
//         } else {
//             self.set(old);
//             false
//         }
//     }
//     fn new() -> Self {
//         VecCell::new(Vec::new())
//     }
// }

impl<T: PartialEq> MatchingCell<T> for RefCell<Option<T>> {
    #[must_use]
    fn try_set(&self, new: T) -> bool {
        let mut guard = self.borrow_mut();
        if guard.as_ref().is_none_or(|old| *old == new) {
            guard.replace(new);
            true
        } else {
            false
        }
    }
    fn new() -> Self {
        RefCell::new(None)
    }
}

trait BatchJoin<T = Self> {
    /// Try to set all elements in `self` to corresponding elements in `others`.
    ///
    /// # Returns
    ///
    /// Returns `true` if all elements were successfully set, `false` otherwise.
    ///
    /// If and only if it returns `true`, all elements in `self` are set to corresponding elements
    /// in `others`, otherwise `self` is unchanged.
    #[must_use]
    fn join(&mut self, others: &T) -> Option<()>;
}

impl<I: Idx, T: BatchJoin<T>> BatchJoin for IndexVec<I, T> {
    #[must_use]
    fn join(&mut self, others: &Self) -> Option<()> {
        debug_assert!(self.len() == others.len());
        for (a, b) in self.iter_mut().zip(others.iter()) {
            a.join(b)?;
        }
        Some(())
    }
}
impl<K: Hash + Eq + Copy, V: BatchJoin<V> + Clone> BatchJoin for FxHashMap<K, V> {
    #[must_use]
    fn join(&mut self, others: &Self) -> Option<()> {
        for (k, v_other) in others.iter() {
            if let Some(v_self) = self.get_mut(&k) {
                v_self.join(v_other)?;
            } else {
                self.insert(*k, v_other.clone());
            }
        }
        Some(())
    }
}
impl<T: PartialEq + Copy> BatchJoin for Cell<Option<T>> {
    #[must_use]
    fn join(&mut self, others: &Self) -> Option<()> {
        if let Some(other) = others.get() {
            if let Some(self_) = self.get() {
                if self_ != other {
                    return None;
                }
            } else {
                self.set(Some(other));
            }
        }
        Some(())
    }
}
impl<T: PartialEq + Clone> BatchJoin for RefCell<Option<T>> {
    #[must_use]
    fn join(&mut self, others: &Self) -> Option<()> {
        let other = others.borrow().as_ref().cloned();
        if let Some(other) = other {
            let mut self_ = self.borrow_mut();
            if let Some(self_val) = self_.as_ref() {
                if self_val != &other {
                    return None;
                }
            } else {
                self_.replace(other);
            }
        }
        Some(())
    }
}
impl<T: BatchJoin> BatchJoin for Option<T> {
    #[must_use]
    fn join(&mut self, others: &Self) -> Option<()> {
        if let (Some(self_), Some(other)) = (self.as_mut(), others.as_ref()) {
            self_.join(other)?;
        } else {
            *self = None;
        }
        Some(())
    }
}
// impl<T: PartialEq + Clone> BatchJoin for VecCell<T> {
//     #[must_use]
//     fn join(&mut self, others: &Self) -> Option<()> {
//         if let Some(other) = others.get() {
//             if let Some(self_) = self.get() {
//                 if &*self_ != &*other {
//                     return None;
//                 }
//             } else {
//                 self.set(other.clone());
//             }
//         }
//         Some(())
//     }
// }

type MatchingBlock = IndexVec<usize, StatementMatches>;
type StatementMatches = RefCell<Option<WithCallStack<StatementMatch>>>;
type LocalMatches = RefCell<Option<WithCallStack<mir::Local>>>;
type TyVarMatches<'tcx> = RefCell<Option<ty::Ty<'tcx>>>;
type ConstVarMatches<'tcx> = RefCell<Option<Const<'tcx>>>;
type PlaceVarMatches<'tcx> = RefCell<Option<WithCallStack<mir::PlaceRef<'tcx>>>>;
type MirStatementBackMatches = IndexVec<usize, MirStatementBackMatch>;
type MirStatementBackMatch = RefCell<Option<Vec1<pat::Location>>>;
type AdtMatches<'tcx> = RefCell<Option<AdtMatch<'tcx>>>;

/// See its counterpart in [`crate::matches`].
// FIXME: use a sparse representation to save memory.
#[derive(Clone, PartialEq)]
struct Matching<'tcx> {
    basic_blocks: IndexVec<pat::BasicBlock, MatchingBlock>,
    locals: IndexVec<pat::Local, LocalMatches>,
    ty_vars: IndexVec<pat::TyVarIdx, TyVarMatches<'tcx>>,
    const_vars: IndexVec<pat::ConstVarIdx, ConstVarMatches<'tcx>>,
    place_vars: IndexVec<pat::PlaceVarIdx, PlaceVarMatches<'tcx>>,
    /// Track which pattern statement the statement is matched to,
    /// so that one statement in MIR can be matched to at most one statement in pattern.
    ///
    /// Set to `None` after propagation.
    mir_statements: IndexVec<mir::BasicBlock, MirStatementBackMatches>,
    /// See [`crate::adt::AdtMatch`].
    adt_matches: FxHashMap<Symbol, AdtMatches<'tcx>>,
}

impl<'tcx> Matching<'tcx> {
    #[must_use]
    fn is_complete(&self) -> bool {
        for bb in self.basic_blocks.iter() {
            for stmt in bb.iter() {
                if stmt.borrow().is_none() {
                    return false;
                }
            }
        }
        for local in self.locals.iter() {
            if local.borrow().is_none() {
                return false;
            }
        }
        for ty_var in self.ty_vars.iter() {
            if ty_var.borrow().is_none() {
                return false;
            }
        }
        for const_var in self.const_vars.iter() {
            if const_var.borrow().is_none() {
                return false;
            }
        }
        for place_var in self.place_vars.iter() {
            if place_var.borrow().is_none() {
                return false;
            }
        }
        for bb in self.mir_statements.iter() {
            for stmt in bb.iter() {
                if stmt.borrow().is_none() {
                    return false;
                }
            }
        }
        true
    }
    #[must_use]
    // #[instrument(level = "trace", skip_all, ret)]
    fn has_statement_intersection(&self, other: &Self) -> bool {
        for (bb_self, bb_other) in self.basic_blocks.iter().zip(other.basic_blocks.iter()) {
            for (stmt_self, stmt_other) in bb_self.iter().zip(bb_other.iter()) {
                if let (Some(_), Some(_)) = (stmt_self.borrow().as_ref(), stmt_other.borrow().as_ref()) {
                    return true;
                }
            }
        }
        false
    }
    fn preserve_reahability(
        &self,
        other: &Self,
        mir_reachability: &Reachability<mir::BasicBlock>,
        pat_reachability: &Reachability<pat::BasicBlock>,
    ) -> bool {
        for (bb_idx_self, bb_self) in self.basic_blocks.iter_enumerated() {
            for (stmt_idx_self, stmt_self) in bb_self.iter().enumerate() {
                if let Some(loc_self) = stmt_self.borrow().as_ref()
                    && let (def_id_self, Some(loc_self)) = loc_self.bottom_location()
                {
                    for (bb_idx_other, bb_other) in other.basic_blocks.iter_enumerated() {
                        for (stmt_idx_other, stmt_other) in bb_other.iter().enumerate() {
                            if let Some(loc_other) = stmt_other.borrow().as_ref()
                                && let (def_id_other, Some(loc_other)) = loc_other.bottom_location()
                            {
                                debug_assert_eq!(def_id_self, def_id_other);
                                let pat_reachability = pat_reachability.is_reachable(
                                    pat::Location {
                                        block: bb_idx_self,
                                        statement_index: stmt_idx_self,
                                    },
                                    pat::Location {
                                        block: bb_idx_other,
                                        statement_index: stmt_idx_other,
                                    },
                                );
                                let mir_reachability = mir_reachability.is_reachable(
                                    mir::Location {
                                        block: loc_self.block,
                                        statement_index: loc_self.statement_index,
                                    },
                                    mir::Location {
                                        block: loc_other.block,
                                        statement_index: loc_other.statement_index,
                                    },
                                );
                                // trace!(
                                //     ?pat_reachability,
                                //     ?mir_reachability,
                                //     "checking reachability preservation"
                                // );
                                if !pat_reachability.covered_by(&mir_reachability) {
                                    return false;
                                }
                            }
                        }
                    }
                }
            }
        }
        true
    }
    // #[instrument(level = "trace", skip_all)]
    fn join(
        &self,
        other: &Self,
        mir_reachability: &Reachability<mir::BasicBlock>,
        pat_reachability: &Reachability<pat::BasicBlock>,
    ) -> Option<Self> {
        if self.has_statement_intersection(other) {
            return None;
        }
        if !self.preserve_reahability(other, mir_reachability, pat_reachability) {
            return None;
        }
        // self.log_matched();
        // other.log_matched();
        let mut matching = self.clone();
        matching.basic_blocks.join(&other.basic_blocks)?;
        matching.locals.join(&other.locals)?;
        matching.ty_vars.join(&other.ty_vars)?;
        matching.const_vars.join(&other.const_vars)?;
        matching.place_vars.join(&other.place_vars)?;
        matching.mir_statements.join(&other.mir_statements)?;
        matching.adt_matches.join(&other.adt_matches)?;
        // matching.log_matched();
        Some(matching)
    }

    #[instrument(level = "trace", skip(self, body))]
    fn propagate(&self, caller_loc: mir::Location, body: &mir::Body<'_>, def_id: LocalDefId) -> Self {
        let mut matching = self.clone();

        for (_bb_idx, bb) in matching.basic_blocks.iter_enumerated() {
            for (_stmt_idx, stmt) in bb.iter_enumerated() {
                if let Some(stmt) = stmt.borrow_mut().as_mut() {
                    // let pat_loc = pat::Location {
                    //     block: bb_idx,
                    //     statement_index: stmt_idx,
                    // };
                    // if pat_loc.block == pat::BasicBlock::from_usize(0) && pat_loc.statement_index == 0 {
                    //     // Propagate to caller location.
                    //     stmt.set(Some(StatementMatch::Location(caller_loc)));
                    // }
                    stmt.push_call(def_id, caller_loc);
                }
            }
        }

        matching.mir_statements = IndexVec::from_fn_n(
            |bb| {
                IndexVec::from_fn_n(
                    |_| MatchingCell::new(),
                    body.basic_blocks[bb].statements.len() + body.basic_blocks[bb].terminator.is_some() as usize,
                )
            },
            body.basic_blocks.len(),
        );

        let pat_stmts = matching
            .mir_statements
            .iter_mut()
            .flat_map(|bb| bb.iter().flat_map(|stmt| stmt.borrow().as_ref().cloned()).flatten());
        let pat_stmts: Vec<pat::Location> = pat_stmts.collect();
        let pat_stmts: Option<Vec1<pat::Location>> = pat_stmts.try_into().ok();
        if let Some(mut pat_stmts) = pat_stmts {
            pat_stmts.sort();
            matching.mir_statements[caller_loc.block][caller_loc.statement_index].set_checked(pat_stmts);
        }
        matching
    }

    fn check(caller_loc: mir::Location, caller_id: LocalDefId, matchings: &AllMatchings<'_, 'tcx>) {
        if cfg!(debug_assertions) {
            let body = matchings[&caller_id].graph.body;
            debug_assert!(body.basic_blocks.len() > caller_loc.block.as_usize());
            let bb = &body.basic_blocks[caller_loc.block];
            let bb_len = bb.statements.len() + bb.terminator.is_some() as usize;
            trace!(
                ?caller_id,
                ?caller_loc,
                ?bb_len,
                bb_num_stmt = ?bb.statements.len(),
                bb_num_term = ?bb.terminator.is_some(),
                "checking caller location validity"
            );
            debug_assert!(
                bb_len > caller_loc.statement_index,
                "{} > {} does not hold",
                bb_len,
                caller_loc.statement_index,
            );
        }
    }

    #[instrument(level = "trace", skip(self), ret)]
    fn to_matched(&self) -> Option<Matched<'tcx>> {
        self.log_matched();
        Some(Matched {
            basic_blocks: self
                .basic_blocks
                .iter()
                .map(|bb| {
                    Some(MatchedBlock {
                        statements: bb
                            .iter()
                            .map(|stmt| stmt.borrow().as_ref().cloned())
                            .collect::<Option<_>>()?,
                    })
                })
                .collect::<Option<_>>()?,
            locals: self
                .locals
                .iter()
                .map(|local| local.borrow().as_ref().cloned())
                .collect::<Option<_>>()?,
            ty_vars: self
                .ty_vars
                .iter()
                .map(|ty_var| ty_var.borrow().as_ref().cloned())
                .collect::<Option<_>>()?,
            const_vars: self
                .const_vars
                .iter()
                .map(|const_var| const_var.borrow().as_ref().cloned())
                .collect::<Option<_>>()?,
            place_vars: self
                .place_vars
                .iter()
                .map(|place_var| place_var.borrow().as_ref().cloned())
                .collect::<Option<_>>()?,
        })
    }

    #[instrument(
        level = "trace",
        skip(self),
        fields(
            num_bbs = ?self.basic_blocks.len(),
            num_locals = ?self.locals.len(),
            num_ty_vars = ?self.ty_vars.len(),
            num_const_vars = ?self.const_vars.len(),
            num_place_vars = ?self.place_vars.len(),
            num_mir_stmts = ?self.mir_statements.len(),
        )
    )]
    fn log_matched(&self) {
        for (bb_idx, bb) in self.basic_blocks.iter_enumerated() {
            for (stmt_idx, stmt) in bb.iter().enumerate() {
                let pat_stmt = pat::Location {
                    block: bb_idx,
                    statement_index: stmt_idx,
                };
                if let Some(matched) = stmt.borrow().as_ref() {
                    trace!(?pat_stmt, ?matched, num_stmts = ?bb.len(), num_bbs = ?self.basic_blocks.len());
                }
            }
        }
        for (pat_local, local) in self.locals.iter_enumerated() {
            if let Some(matched) = local.borrow().as_ref() {
                trace!(?pat_local, ?matched, num_locals = ?self.locals.len());
            }
        }
        for (pat_ty_var, ty_var) in self.ty_vars.iter_enumerated() {
            if let Some(matched) = ty_var.borrow().as_ref() {
                trace!(?pat_ty_var, ?matched, num_ty_vars = ?self.ty_vars.len());
            }
        }
        for (pat_const_var, const_var) in self.const_vars.iter_enumerated() {
            if let Some(matched) = const_var.borrow().as_ref() {
                trace!(?pat_const_var, ?matched, num_const_vars = ?self.const_vars.len());
            }
        }
        for (pat_place_var, place_var) in self.place_vars.iter_enumerated() {
            if let Some(matched) = place_var.borrow().as_ref() {
                trace!(?pat_place_var, ?matched, num_place_vars = ?self.place_vars.len());
            }
        }
        for (bb_idx, bb) in self.mir_statements.iter_enumerated() {
            for (stmt_idx, stmt) in bb.iter_enumerated() {
                let loc = mir::Location {
                    block: bb_idx,
                    statement_index: stmt_idx,
                };
                if let Some(matched) = stmt.borrow().as_ref() {
                    trace!(?loc, matched = ?&*matched, num_stmts = ?bb.len(), num_bbs = ?self.mir_statements.len());
                }
            }
        }
    }
}

macro_rules! impl_index {
    ($param:ident : $idx:ty => $output:ty = $($expr:tt)+) => {
        impl<'tcx> Index<$idx> for Matching<'tcx> {
            type Output = $output;

            fn index(&self, $param: $idx) -> &Self::Output {
                &self.$($expr)+
            }
        }

        // impl IndexMut<$idx> for Matching<'_> {
        //     fn index_mut(&mut self, $param: $idx) -> &mut Self::Output {
        //         &mut self.$($expr)+
        //     }
        // }
    };
}

impl_index!(bb:        pat::BasicBlock  => MatchingBlock         = basic_blocks[bb]);
impl_index!(stmt:      pat::Location    => StatementMatches      = basic_blocks[stmt.block][stmt.statement_index]);
impl_index!(local:     pat::Local       => LocalMatches          = locals[local]);
impl_index!(ty_var:    pat::TyVarIdx    => TyVarMatches<'tcx>    = ty_vars[ty_var]);
impl_index!(const_var: pat::ConstVarIdx => ConstVarMatches<'tcx> = const_vars[const_var]);
impl_index!(place_var: pat::PlaceVarIdx => PlaceVarMatches<'tcx> = place_vars[place_var]);
impl_index!(stmt:      mir::Location    => MirStatementBackMatch = mir_statements[stmt.block][stmt.statement_index]);
impl_index!(name:      Symbol           => AdtMatches<'tcx>      = adt_matches[&name]);

/// Experimental matching algorithm interface.
///
/// Algorithm steps:
///
/// - Analyze patterns and build pattern graphs (CFG + DDG).
/// - Analyze MIR and build MIR graphs (CFG + DDG).
/// - Find all possible matches of 1-component in pattern graph to MIR graph.
/// - Continue the following steps for `k` from `1` to `N-1` (number of components in pattern
///   graph):
///   - Propagate matches of `k` components to its callers in the pattern graph.
///   - For each possible match of `k` components and each possible match of `1` components, try to
///     extend it to `k+1` components by adding one more component, if the following holds:
///     - the `k` component and the `1` component is not overlapping.
///     - the new `k+1` component is connected to the existing `k` components in both graphs.
///
/// See the following traits:
///
/// - [`MatchStatement`]
/// - [`MatchTy`]
///
/// [`MatchTy`]: crate::ty::MatchTy
/// [`MatchStatement`]: crate::statement::MatchStatement
#[instrument(level = "trace", skip_all, fields(pat_name = ?pat_name, fn_name = ?fn_pat.name))]
pub fn check2<'a, 'pcx, 'tcx: 'a>(
    tcx: TyCtxt<'tcx>,
    pcx: PatCtxt<'pcx>,
    pat: &'pcx pat::RustItems<'pcx>,
    pat_name: Symbol,
    pat_cfg: &'a PatControlFlowGraph,
    pat_ddg: &'a PatDataDepGraph,
    fn_pat: &'a pat::FnPattern<'pcx>,
    fns: &'a [MirGraph<'tcx>],
) -> Vec<NormalizedMatched<'tcx>> {
    trace!(?pat_name, ?fn_pat.name, fn_count = ?fns.len(), "check2");
    let places = pat.meta.place_vars.iter().map(|var| var.ty).collect();
    let Some(body) = fn_pat.body else {
        debug!("function pattern has no body, returning empty matches");
        return Vec::new();
    };
    let reachability = Reachability::<pat::BasicBlock>::new_pat(body);
    let cx = MatchCtxt2 {
        tcx,
        pcx,
        pat,
        pat_name,
        fn_pat,
        pat_cfg,
        pat_ddg,
        reachability: &reachability,
        places,
        fns,
    };
    let matches_1 = cx.find_matches_1();
    let mut all_matches = matches_1.clone();
    let num_nodes = fn_pat.body.as_ref().map_or(0, |body| body.num_nodes());
    trace!(num_nodes, "starting join iterations");
    log_matchings(&matches_1, "matches_1");
    for k in 1..num_nodes {
        // Join matches of `k` components with matches of `1` components to form matches of `k+1`
        // components.
        let _guard = trace_span!("joining matches", k, num_nodes, num_fns = all_matches.len()).entered();
        if all_matches.values().all(|matches| matches.matches.is_empty()) {
            trace!("no more matches to join, stopping early");
            break;
        }
        all_matches = cx.join_matches(&all_matches, &matches_1);
        cx.propagate(&mut all_matches);
    }
    // Now all_matches contains all possible matches of full pattern graph to MIR graphs.
    let mut results = Vec::new();
    for fn_graph in fns {
        if let Some(matchings) = all_matches.get(&fn_graph.id) {
            for matching in matchings.matches.iter() {
                if let Some(matched) = matching.to_matched() {
                    debug_span!("check2", ?fn_graph.id, ?pat_name, ?fn_pat.name).in_scope(|| {
                        trace!("found full match for function");
                        matched.log_matched();
                    });
                    let bottom = fn_graph.id;
                    let label_map = &fn_pat.expect_body().labels;
                    let attr_map = fn_pat.extra_span(tcx, bottom).unwrap();
                    let matched = NormalizedMatched::new(bottom, matched, label_map, &attr_map);
                    results.push(matched);
                }
            }
        }
    }
    let results = FxHashSet::from_iter(results).into_iter().collect::<Vec<_>>();
    debug!(match_count = ?results.len(), "check2 done");
    results
}
