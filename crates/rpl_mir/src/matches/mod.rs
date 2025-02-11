use std::cell::Cell;
use std::fmt;
use std::ops::Index;

use rpl_match::CountedMatch;
use rpl_mir_graph::TerminatorEdges;
use rustc_data_structures::fx::FxIndexSet;
use rustc_data_structures::stack::ensure_sufficient_stack;
use rustc_index::bit_set::HybridBitSet;
use rustc_index::{Idx, IndexVec};
use rustc_middle::mir::visit::{MutatingUseContext, PlaceContext};
use rustc_middle::mir::{self};
use rustc_middle::ty::Ty;
use rustc_span::Span;

use crate::{pat, CheckMirCtxt};

pub struct Matched<'tcx> {
    pub basic_blocks: IndexVec<pat::BasicBlock, MatchedBlock>,
    pub locals: IndexVec<pat::Local, mir::Local>,
    pub ty_vars: IndexVec<pat::TyVarIdx, Ty<'tcx>>,
}

pub struct MatchedBlock {
    pub statements: Vec<StatementMatch>,
    pub start: Option<mir::BasicBlock>,
    pub end: Option<mir::BasicBlock>,
}

impl Index<pat::BasicBlock> for Matched<'_> {
    type Output = MatchedBlock;

    fn index(&self, bb: pat::BasicBlock) -> &Self::Output {
        &self.basic_blocks[bb]
    }
}

impl Index<pat::Location> for Matched<'_> {
    type Output = StatementMatch;

    fn index(&self, stmt: pat::Location) -> &Self::Output {
        &self.basic_blocks[stmt.block].statements[stmt.statement_index]
    }
}

impl Index<pat::Local> for Matched<'_> {
    type Output = mir::Local;

    fn index(&self, local: pat::Local) -> &Self::Output {
        &self.locals[local]
    }
}

impl<'tcx> Index<pat::TyVarIdx> for Matched<'tcx> {
    type Output = Ty<'tcx>;

    fn index(&self, ty_var: pat::TyVarIdx) -> &Self::Output {
        &self.ty_vars[ty_var]
    }
}

pub fn matches<'tcx>(cx: &CheckMirCtxt<'_, '_, 'tcx>) -> Vec<Matched<'tcx>> {
    let mut matching = MatchCtxt::new(cx);
    matching.do_match();
    matching.matched.take()
}

#[derive(Debug)]
struct Matching<'tcx> {
    basic_blocks: IndexVec<pat::BasicBlock, MatchingBlock>,
    locals: IndexVec<pat::Local, LocalMatches>,
    ty_vars: IndexVec<pat::TyVarIdx, TyVarMatches<'tcx>>,
}

impl Index<pat::BasicBlock> for Matching<'_> {
    type Output = MatchingBlock;

    fn index(&self, bb: pat::BasicBlock) -> &Self::Output {
        &self.basic_blocks[bb]
    }
}

impl Index<pat::Location> for Matching<'_> {
    type Output = StatementMatches;

    fn index(&self, stmt: pat::Location) -> &Self::Output {
        &self.basic_blocks[stmt.block].statements[stmt.statement_index]
    }
}

impl Index<pat::Local> for Matching<'_> {
    type Output = LocalMatches;

    fn index(&self, local: pat::Local) -> &Self::Output {
        &self.locals[local]
    }
}

impl<'tcx> Index<pat::TyVarIdx> for Matching<'tcx> {
    type Output = TyVarMatches<'tcx>;

    fn index(&self, ty_var: pat::TyVarIdx) -> &Self::Output {
        &self.ty_vars[ty_var]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StatementMatch {
    Arg(mir::Local),
    Location(mir::Location),
}

impl From<mir::Local> for StatementMatch {
    fn from(local: mir::Local) -> Self {
        StatementMatch::Arg(local)
    }
}

impl From<mir::Location> for StatementMatch {
    fn from(loc: mir::Location) -> Self {
        StatementMatch::Location(loc)
    }
}

impl fmt::Debug for StatementMatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StatementMatch::Arg(local) => local.fmt(f),
            StatementMatch::Location(loc) => loc.fmt(f),
        }
    }
}

impl StatementMatch {
    // fn is_in_block(self, block: mir::BasicBlock) -> bool {
    //     match self {
    //         StatementMatch::Arg(_) => true,
    //         StatementMatch::Location(loc) => loc.block == block,
    //     }
    // }
    fn expect_location(&self) -> mir::Location {
        match self {
            StatementMatch::Location(loc) => *loc,
            _ => panic!("expect location"),
        }
    }

    pub fn debug_with<'a, 'tcx>(self, body: &'a mir::Body<'tcx>) -> impl core::fmt::Debug + use<'a, 'tcx> {
        struct DebugStatementMatch<'a, 'tcx> {
            stmt_match: StatementMatch,
            body: &'a mir::Body<'tcx>,
        }
        impl core::fmt::Debug for DebugStatementMatch<'_, '_> {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                match self.stmt_match {
                    StatementMatch::Arg(local) => write!(f, "let {local:?}: {:?}", self.body.local_decls[local].ty),
                    StatementMatch::Location(location) => self.body.stmt_at(location).either_with(
                        f,
                        |f, stmt| write!(f, "{location:?}: {stmt:?}"),
                        |f, terminator| write!(f, "{location:?}: {:?}", terminator.kind),
                    ),
                }
            }
        }
        DebugStatementMatch { stmt_match: self, body }
    }

    pub fn source_info<'a>(self, body: &'a mir::Body<'_>) -> &'a mir::SourceInfo {
        match self {
            StatementMatch::Arg(arg) => &body.local_decls[arg].source_info,
            StatementMatch::Location(loc) => body.source_info(loc),
        }
    }

    pub fn span_no_inline(self, body: &mir::Body<'_>) -> Span {
        let source_info = self.source_info(body);
        let mut scope = source_info.scope;
        while let Some(parent_scope) = body.source_scopes[scope].inlined_parent_scope {
            scope = parent_scope;
        }
        if let Some((_instance, span)) = body.source_scopes[scope].inlined {
            return span;
        }
        source_info.span
    }
}

struct MatchCtxt<'a, 'pcx, 'tcx> {
    cx: &'a CheckMirCtxt<'a, 'pcx, 'tcx>,
    matching: Matching<'tcx>,
    matched: Cell<Vec<Matched<'tcx>>>,
}

impl<'a, 'pcx, 'tcx> MatchCtxt<'a, 'pcx, 'tcx> {
    fn new(cx: &'a CheckMirCtxt<'a, 'pcx, 'tcx>) -> Self {
        Self {
            cx,
            matching: Self::new_checking(cx),
            matched: Cell::new(Vec::new()),
        }
    }
    fn new_checking(cx: &'a CheckMirCtxt<'a, 'pcx, 'tcx>) -> Matching<'tcx> {
        let num_blocks = cx.mir_pat.basic_blocks.len();
        let num_locals = cx.mir_pat.locals.len();
        Matching {
            basic_blocks: IndexVec::from_fn_n(
                |bb_pat| {
                    let mut num_stmt_pats = cx.mir_pat[bb_pat].num_statements_and_terminator();
                    // We don't need to match the end of the pattern, because it is only a marker and has no
                    // corresponding terminator.
                    if cx.mir_pat[bb_pat].has_pat_end() {
                        num_stmt_pats -= 1;
                    }
                    MatchingBlock::new(num_stmt_pats)
                },
                num_blocks,
            ),
            locals: IndexVec::from_fn_n(|_| LocalMatches::new(cx.body.local_decls.len()), num_locals),
            ty_vars: IndexVec::from_fn_n(|_| TyVarMatches::new(), cx.fn_pat.meta.ty_vars.len()),
        }
    }
    #[instrument(level = "debug", skip(self))]
    fn build_candidates(&mut self) {
        for (bb_pat, block_mat) in self.matching.basic_blocks.iter_enumerated_mut() {
            let _span = debug_span!("build_candidates", ?bb_pat).entered();
            let block_pat = &self.cx.mir_pat[bb_pat];
            for (stmt_pat, matches) in block_mat.statements.iter_mut().enumerate() {
                let loc_pat = (bb_pat, stmt_pat).into_location();
                let _span = debug_span!(
                    "build_candidates",
                    ?loc_pat,
                    stmt_pat = ?self.cx.mir_pat[bb_pat].debug_stmt_at(stmt_pat),
                )
                .entered();
                // Note that this should be outside of the `self.cx.body.basic_blocks.iter_enumerated()` loop to
                // avoid duplicated argument candidates.
                if loc_pat.statement_index < block_pat.statements.len()
                    && let pat::StatementKind::Assign(
                        pat::Place {
                            local: local_pat,
                            projection: [],
                        },
                        pat::Rvalue::Any,
                    ) = block_pat.statements[loc_pat.statement_index]
                {
                    if self.cx.mir_pat.self_idx == Some(local_pat) {
                        let self_value = mir::Local::from_u32(1);
                        if self.cx.match_local(local_pat, self_value) {
                            info!(
                                "candidate matched: {loc_pat:?} (self) {pat:?} <-> {self_value:?}",
                                pat = self.cx.mir_pat[bb_pat].debug_stmt_at(stmt_pat),
                            );

                            matches.candidates.push(StatementMatch::Arg(self_value));
                        }
                    } else {
                        for arg in self.cx.body.args_iter() {
                            let _span = debug_span!("build_candidates", arg = ?StatementMatch::Arg(arg).debug_with(self.cx.body))
                                .entered();
                            if self.cx.match_local(local_pat, arg) {
                                info!(
                                    "candidate matched: {loc_pat:?} {pat:?} <-> {arg:?}",
                                    pat = self.cx.mir_pat[bb_pat].debug_stmt_at(stmt_pat),
                                );
                                matches.candidates.push(StatementMatch::Arg(arg));
                            }
                        }
                    }
                }
                for (bb, block) in self.cx.body.basic_blocks.iter_enumerated() {
                    let _span = debug_span!("build_candidates", ?bb).entered();
                    for stmt in 0..=block.statements.len() {
                        let loc = (bb, stmt).into_location();
                        let _span =
                            debug_span!("build_candidates", stmt = ?StatementMatch::Location(loc).debug_with(self.cx.body))
                                .entered();
                        if self.cx.match_statement_or_terminator(loc_pat, loc) {
                            info!(
                                "candidate matched: {loc_pat:?} {pat:?} <-> {statement:?}",
                                pat = self.cx.mir_pat[bb_pat].debug_stmt_at(stmt_pat),
                                statement = StatementMatch::Location(loc).debug_with(self.cx.body),
                            );
                            matches.candidates.push(StatementMatch::Location(loc));
                        }
                    }
                }
            }
        }
        for ((local_pat, candidates), matches) in
            core::iter::zip(self.cx.locals.iter_enumerated(), &mut self.matching.locals)
        {
            matches.candidates = std::mem::replace(
                &mut *candidates.borrow_mut(),
                HybridBitSet::new_empty(self.cx.body.local_decls.len()),
            );
            if matches.candidates.is_empty() {
                continue;
            }
            // If the local variable is the `self` parameter or the `RET` place, we only need to match the
            // corresponding local variable in the MIR graph.
            let only_candidate = if self.cx.mir_pat.self_idx == Some(local_pat) {
                mir::Local::from_u32(1)
            } else if self.cx.mir_pat.return_idx == Some(local_pat) {
                mir::RETURN_PLACE
            } else {
                continue;
            };
            let has_only_candidate = matches.candidates.remove(only_candidate);
            matches.candidates.clear();
            if has_only_candidate {
                matches.candidates.insert(only_candidate);
            }
        }
        for (candidates, matches) in core::iter::zip(&self.cx.ty.ty_vars, &mut self.matching.ty_vars) {
            matches.candidates = std::mem::take(&mut *candidates.borrow_mut());
        }
    }
    #[instrument(level = "info", skip(self))]
    fn do_match(&mut self) {
        self.build_candidates();
        self.matching.log_candidates();
        if self.matching.has_empty_candidates(self.cx) {
            return;
        }
        self.match_candidates();
    }
    // Recursvely traverse all candidates of type variables, local variables, and statements, and then
    // match the graph.
    #[instrument(level = "info", skip(self))]
    fn match_candidates(&self) {
        let loc_pats = self.loc_pats().collect::<Vec<_>>();
        self.match_ty_var_candidates(pat::TyVarIdx::ZERO, &loc_pats);
    }
    fn match_ty_var_candidates(&self, ty_var: pat::TyVarIdx, loc_pats: &[pat::Location]) {
        if ty_var == self.cx.fn_pat.meta.ty_vars.next_index() {
            self.match_local_candidates(pat::Local::ZERO, loc_pats);
            return;
        }
        for &cand in &self.matching[ty_var].candidates {
            let _span = debug_span!("match_ty_var_candidates", ?ty_var, ?cand).entered();
            if self.match_ty_var(ty_var, cand) {
                // recursion
                ensure_sufficient_stack(|| self.match_ty_var_candidates(ty_var.plus(1), loc_pats));
            }
            // backtrack, clear status
            self.unmatch_ty_var(ty_var);
        }
    }
    fn match_local_candidates(&self, local: pat::Local, loc_pats: &[pat::Location]) {
        if local == self.cx.mir_pat.locals.next_index() {
            self.match_stmt_candidates(loc_pats);
            return;
        }
        for cand in self.matching[local].candidates.iter() {
            let _span = debug_span!("match_local_candidates", ?local, ?cand).entered();
            if self.match_local(local, cand) {
                // recursion
                ensure_sufficient_stack(|| self.match_local_candidates(local.plus(1), loc_pats));
            }
            // backtrack, clear status
            self.unmatch_local(local);
        }
    }
    fn match_stmt_candidates(&self, loc_pats: &[pat::Location]) {
        let Some((&loc_pat, loc_pats)) = loc_pats.split_first() else {
            if self.match_graph() {
                self.matching.log_matched(self.cx);
                let mut matched = self.matched.take();
                matched.push(self.matching.to_matched());
                self.matched.set(matched);
            }
            return;
        };
        for &cand in &self.matching[loc_pat].candidates {
            let _span = debug_span!("match_stmt_candidate", ?loc_pat, ?cand).entered();
            if self.match_stmt(loc_pat, cand) {
                // recursion
                ensure_sufficient_stack(|| self.match_stmt_candidates(loc_pats));
            }
            // backtrack, clear status
            self.unmatch_stmt(loc_pat);
        }
    }

    #[instrument(level = "info", skip(self), ret)]
    fn match_graph(&self) -> bool {
        for block in &self.matching.basic_blocks {
            block.start.take();
            block.end.take();
        }
        self.match_cfg() && self.match_ddg()
    }

    #[instrument(level = "info", skip(self), ret)]
    fn match_cfg(&self) -> bool {
        self.match_block(pat::BasicBlock::ZERO)
    }

    #[instrument(level = "info", skip(self), ret)]
    fn match_ddg(&self) -> bool {
        self.loc_pats().all(|loc_pat| {
            let StatementMatch::Location(loc) = self.matching[loc_pat].force_get_matched() else {
                return true;
            };
            let matched = self.match_stmt_deps(
                self.cx.pat_ddg.deps(loc_pat.block, loc_pat.statement_index),
                |dep_loc, local| {
                    self.cx
                        .mir_ddg
                        .get_dep(loc.block, loc.statement_index, dep_loc.block, dep_loc.statement_index)
                        == Some(local)
                },
            );
            debug!(?loc_pat, ?loc, ?matched, "match_stmt_deps");
            matched
        })
    }

    fn loc_pats(&self) -> impl Iterator<Item = pat::Location> + use<'_> {
        self.matching
            .basic_blocks
            .iter_enumerated()
            .flat_map(|(bb, block)| (0..block.statements.len()).map(move |stmt| (bb, stmt).into_location()))
    }

    #[instrument(level = "debug", skip(self), ret)]
    fn match_block(&self, bb_pat: pat::BasicBlock) -> bool {
        if self.cx.mir_pat[bb_pat].has_pat_end() {
            return true;
        }
        let block = self.matching[bb_pat]
            .terminator()
            .force_get_matched()
            .expect_location()
            .block;
        self.match_block_ends_with(bb_pat, block)
    }
    #[instrument(level = "debug", skip(self), ret)]
    fn match_block_starts_with(&self, bb_pat: pat::BasicBlock, bb: mir::BasicBlock) -> bool {
        let matching = &self.matching[bb_pat];
        matching.start.get().is_some_and(|block| block == bb)
            || matching.start.get().is_none()
                && self.match_stmt_deps(
                    self.cx.pat_ddg[bb_pat]
                        .rdep_start()
                        .map(|(stmt_pat, local_pat)| ((bb_pat, stmt_pat).into_location(), local_pat)),
                    |dep_loc, local| {
                        dep_loc.block == bb && self.cx.mir_ddg[bb].is_rdep_start(dep_loc.statement_index, local)
                            || dep_loc.block != bb && self.cx.mir_ddg[bb].is_rdep_start_end(local)
                    },
                )
                && {
                    matching.start.set(Some(bb));
                    // Since start and the end of a block in the pattern graph may match different blocks
                    // in the MIR graph, we don't use `bb` here.
                    ensure_sufficient_stack(|| self.match_block(bb_pat))
                }
            || {
                matching.start.set(None);
                false
            }
    }
    // FIXME: possibly missing control dependency edges, and low efficiency.
    // For intrablock edges, we can directly test if it is an edge of DDG, but for interblock edges, we
    // need to recursively check if there is a path from the start of the block `bb` to location
    // `rdep_loc`, because we don't store the interblock edges from the start of blocks yet.
    #[instrument(level = "debug", skip(self), ret)]
    fn is_rdep_start(&self, bb: mir::BasicBlock, rdep_loc: mir::Location, local: mir::Local) -> bool {
        rdep_loc.block == bb && self.cx.mir_ddg[bb].is_rdep_start(rdep_loc.statement_index, local)
            || self.cx.mir_ddg[bb].is_rdep_start_end(local)
                && self.cx.mir_cfg[bb]
                    .successors()
                    .any(|bb| ensure_sufficient_stack(|| self.is_rdep_start(bb, rdep_loc, local)))
    }
    // FIXME: in pattern like CVE-2021-29941/2/pattern_uninitialized_slice_mut, when there is a
    // statement in the pattern block matching a terminator, like this
    // ```
    // // pattern
    // ?bb0: {
    //     let len: usize = _;
    //     let vec: Vec<u32> = Vec::with_capacity(len);
    // }
    // ?bb1: {
    //     let vec_ptr = vec.as_mut_ptr();
    // }
    // ?bb2: {
    //     let arr: &mut [u32] = std::slice::from_raw_parts_mut(vec_ptr, len);
    // }
    //
    // // code
    // bb0: {
    //     let vec = Vec::with_capacity(len);
    // }
    // bb1: {
    //     let vec_ptr = vec.as_mut_ptr();
    // }
    // bb2: {
    //     let len = bla.len();
    // }
    // bb3: {
    //     let arr: &mut [u32] = std::slice::from_raw_parts_mut(vec_ptr, len);
    // }
    // ```
    // where `Vec::with_capacity` happens in advance of `bla.len()`, since the current implementation
    // of `match_block_ends_with` only tries to match `?bb0` with `bb0`, it will fail to match the
    // `bla.len()` statement in `bb3` with `?bb2` due to no data dependency edge can be found from
    // `bla.len()` to the end of `bb0`.
    #[instrument(level = "debug", skip(self), ret)]
    fn match_block_ends_with(&self, bb_pat: pat::BasicBlock, bb: mir::BasicBlock) -> bool {
        // FIXME: handle empty blocks
        if self.cx.mir_pat[bb_pat].statements.is_empty()
            && matches!(self.cx.mir_pat[bb_pat].terminator(), pat::TerminatorKind::Goto(_))
        {
            return true;
        }
        let matching = &self.matching[bb_pat];
        matching.end.get().is_some_and(|block| block == bb)
            || matching.end.get().is_none()
                // FIXME: handle move of return value
                && self.match_stmt_deps(self.cx.pat_ddg.dep_end(bb_pat), |dep_loc, local| {
                    self.cx.mir_ddg.get_dep_end(bb, dep_loc.block, dep_loc.statement_index)
                        .map(|dep_end| dep_end == local).unwrap_or(true)
                })
                && {
                    matching.end.set(Some(bb));
                    // recursively check all the succesor blocks
                    self.match_block_successors(bb_pat, bb)
                }
            || {
                matching.end.set(None);
                false
            }
    }

    /// Match DDG edges of a statement, or the start or end of a block (See 3 callers of this
    /// function).
    ///
    /// We iterate over all data dependencies of a statement (i.e. the iterator
    /// `pat_deps`), and for each dependency `dep_loc_pat` we try to test whether dependency
    /// edge (`local_pat`) of the pattern DDG matches that of the MIR DDG (`local`).
    ///
    /// ```text
    /// 1. dependencies of a statement
    /// dep_loc_pat -----> dep_loc
    ///   ^                   ^
    ///   | local_pat         | local
    ///   |                   |
    /// loc_pat -----------> loc
    ///
    /// 2. dependencies of the end of a block
    /// dep_loc_pat -----> dep_loc
    ///   ^                   ^
    ///   | local_pat         | local
    ///   |                   |
    /// block_end_pat ---> block_end
    ///
    /// 3. reversed dependencies of the start of a block
    /// block_start_pat ---> block_start
    ///   ^                   ^
    ///   | local_pat         | local
    ///   |                   |
    /// rdep_loc_pat -----> rdep_loc
    /// ```
    fn match_stmt_deps(
        &self,
        mut pat_deps: impl Iterator<Item = (impl IntoLocation<Location = pat::Location>, pat::Local)>,
        mut match_dep_local: impl FnMut(mir::Location, mir::Local) -> bool,
    ) -> bool {
        pat_deps.all(|(dep_loc_pat, local_pat)| {
            let dep_loc_pat = dep_loc_pat.into_location();
            let local = self.matching[local_pat].force_get_matched();
            let dep_stmt = self.matching[dep_loc_pat].force_get_matched();
            let matched = match dep_stmt {
                StatementMatch::Arg(l) => l == local,
                StatementMatch::Location(dep_loc) => match_dep_local(dep_loc, local),
            };
            debug!(
                matched,
                "match_stmt_deps: {dep_loc_pat:?} <-> {dep_stmt:?}, {local_pat:?} <-> {local:?}",
            );
            matched
        })
    }
    #[instrument(level = "debug", skip(self), ret)]
    fn match_block_successors(&self, bb_pat: pat::BasicBlock, bb: mir::BasicBlock) -> bool {
        use TerminatorEdges::{AssignOnReturn, Double, Single, SwitchInt};
        debug!(term_pat = ?self.cx.pat_cfg[bb_pat], term = ?self.cx.mir_cfg[bb]);
        match (&self.cx.pat_cfg[bb_pat], &self.cx.mir_cfg[bb]) {
            (TerminatorEdges::None, _) => true,
            (&Single(bb_pat), &Single(bb) | &Double(bb, _)) => self.match_block_starts_with(bb_pat, bb),
            (&Double(bb_pat, unwind_pat), &Double(bb, unwind)) => {
                self.match_block_starts_with(bb_pat, bb) && self.match_block_starts_with(unwind_pat, unwind)
            },
            (
                AssignOnReturn {
                    return_: box ref return_pat,
                    cleanup: cleanup_pat,
                },
                AssignOnReturn {
                    box ref return_,
                    cleanup,
                },
            ) => {
                return_pat.len() == return_.len()
                    && core::iter::zip(return_pat, return_)
                        .chain(cleanup_pat.as_ref().zip(cleanup.as_ref()))
                        .all(|(&bb_pat, &bb)| self.match_block_starts_with(bb_pat, bb))
            },
            (SwitchInt(targets_pat), SwitchInt(targets)) => {
                targets_pat.targets.iter().all(|(&value_pat, &bb_pat)| {
                    targets
                        .targets
                        .get(&value_pat)
                        .is_some_and(|&bb| self.match_block_starts_with(bb_pat, bb))
                }) && match (targets_pat.otherwise, targets.otherwise) {
                    (None, None | Some(_)) => true,
                    (Some(bb_pat), Some(bb)) => self.match_block_starts_with(bb_pat, bb),
                    (Some(_), None) => false,
                }
            },
            _ => false,
        }
    }

    fn match_stmt(&self, loc_pat: pat::Location, stmt_match: StatementMatch) -> bool {
        self.match_stmt_locals(loc_pat, stmt_match) && {
            self.matching[loc_pat].matched.set(Some(stmt_match));
            true
        }
    }
    fn unmatch_stmt(&self, loc_pat: pat::Location) {
        self.unmatch_stmt_adt_matches(loc_pat);
        // self.unmatch_stmt_locals(loc_pat);
        self.matching[loc_pat].matched.set(None);
    }
    #[instrument(level = "debug", skip(self), ret)]
    fn match_stmt_locals(&self, loc_pat: pat::Location, stmt_match: StatementMatch) -> bool {
        let accesses_pat = self.cx.pat_ddg[loc_pat.block].accesses(loc_pat.statement_index);
        let accesses = match stmt_match {
            StatementMatch::Arg(local) => &[(local, PlaceContext::MutatingUse(MutatingUseContext::Store))],
            StatementMatch::Location(loc) => self.cx.mir_ddg[loc.block].accesses(loc.statement_index),
        };
        if loc_pat.statement_index < self.cx.mir_pat[loc_pat.block].statements.len()
            && let pat::StatementKind::Assign(
                pat::Place {
                    local: local_pat,
                    projection: [],
                },
                pat::Rvalue::Any,
            ) = self.cx.mir_pat[loc_pat.block].statements[loc_pat.statement_index]
        {
            return accesses
                .iter()
                .find(|&&(_, access)| access.is_place_assignment())
                // .is_some_and(|&(local, _)| self.match_local(local_pat, local));
                .is_some_and(|&(local, _)| self.matching[local_pat].force_get_matched() == local);
        }
        let mut iter = accesses.iter();
        accesses_pat.iter().all(|&(local_pat, access_pat)| {
            debug!(?local_pat, ?access_pat);
            iter.by_ref()
                .inspect(|&&(local, access)| debug!(?local, ?access))
                .find(|&&(_, access)| access == access_pat)
                // .is_some_and(|&(local, _)| self.match_local(local_pat, local))
                .is_some_and(|&(local, _)| self.matching[local_pat].force_get_matched() == local)
        })
    }
    // Note this is different from `self.cx.match_local`, because we would store the matched result in
    // this method.
    #[instrument(level = "debug", skip(self), ret)]
    fn match_local(&self, local_pat: pat::Local, local: mir::Local) -> bool {
        if self.matching[local_pat].matched.r#match(local) {
            self.log_local_matched(local_pat, local);
        } else {
            self.log_local_conflicted(local_pat, local);
            return false;
        }
        self.match_local_ty(self.cx.mir_pat.locals[local_pat], self.cx.body.local_decls[local].ty)
    }
    #[instrument(level = "debug", skip(self), ret)]
    fn match_local_ty(&self, ty_pat: pat::Ty<'pcx>, ty: Ty<'tcx>) -> bool {
        self.cx.ty.match_ty(ty_pat, ty)
            && self.cx.ty.ty_vars.iter_enumerated().all(|(ty_var, tys)| {
                let tys = core::mem::take(&mut *tys.borrow_mut());
                trace!("type variable {ty_var:?} candidates: {tys:?}",);
                let ty = match tys {
                    tys if tys.is_empty() => return true,
                    tys if tys.len() == 1 => tys.iter().copied().next().unwrap(),
                    tys => {
                        info!("multiple candidates for type variable {ty_var:?}: {tys:?}",);
                        return false;
                    },
                };
                let ty_var_matched = self.matching[ty_var].force_get_matched();
                trace!("type variable {ty_var:?} matched: {ty_var_matched:?} matching: {ty:?}",);
                // self.match_ty_var(ty_var, ty)
                ty_var_matched == ty
            })
    }
    #[instrument(level = "debug", skip(self), ret)]
    fn match_ty_var(&self, ty_var: pat::TyVarIdx, ty: Ty<'tcx>) -> bool {
        self.matching[ty_var].matched.r#match(ty)
    }
    // #[instrument(level = "debug", skip(self))]
    // fn unmatch_stmt_locals(&self, loc_pat: pat::Location) {
    //     for &(local_pat, _) in self.cx.pat_ddg[loc_pat.block].accesses(loc_pat.statement_index) {
    //         self.unmatch_local(local_pat);
    //     }
    // }
    fn unmatch_stmt_adt_matches(&self, loc_pat: pat::Location) {
        let Some(StatementMatch::Location(loc)) = self.matching[loc_pat].matched.get() else {
            return;
        };
        use mir::visit::Visitor;
        use pat::visitor::PatternVisitor;
        struct CollectPlaces<P> {
            places: Vec<P>,
        }
        impl<'pcx> PatternVisitor<'pcx> for CollectPlaces<pat::Place<'pcx>> {
            fn visit_place(&mut self, place: pat::Place<'pcx>, pcx: PlaceContext, loc: pat::Location) {
                self.places.push(place);
                self.super_place(place, pcx, loc);
            }
        }
        impl<'tcx> Visitor<'tcx> for CollectPlaces<mir::Place<'tcx>> {
            fn visit_place(&mut self, &place: &mir::Place<'tcx>, pcx: PlaceContext, loc: mir::Location) {
                self.places.push(place);
                self.super_place(&place, pcx, loc);
            }
        }
        let mut place_pats = CollectPlaces::<pat::Place<'_>> { places: Vec::new() };
        let mut places = CollectPlaces::<mir::Place<'_>> { places: Vec::new() };
        self.cx.mir_pat.stmt_at(loc_pat).either_with(
            &mut place_pats,
            |place_pats, statement| place_pats.visit_statement(statement, loc_pat),
            |place_pats, terminator| place_pats.visit_terminator(terminator, loc_pat),
        );
        self.cx.body.stmt_at(loc).either_with(
            &mut places,
            |places, statement| places.visit_statement(statement, loc),
            |places, terminator| places.visit_terminator(terminator, loc),
        );
        for (place_pat, place) in core::iter::zip(place_pats.places, places.places) {
            self.cx.unmatch_place(place_pat, place);
        }
    }

    #[instrument(level = "debug", skip(self))]
    fn unmatch_local(&self, local_pat: pat::Local) {
        self.matching[local_pat].matched.unmatch();
    }

    #[instrument(level = "debug", skip(self))]
    fn unmatch_ty_var(&self, ty_var: pat::TyVarIdx) {
        self.matching[ty_var].matched.unmatch();
    }

    fn log_stmt_matched(&self, loc_pat: impl IntoLocation<Location = pat::Location>, stmt_match: StatementMatch) {
        let loc_pat = loc_pat.into_location();
        debug!(
            "statement matched {loc_pat:?} {pat:?} <-> {stmt_match:?} {statement:?}",
            pat = self.cx.mir_pat[loc_pat.block].debug_stmt_at(loc_pat.statement_index),
            statement = stmt_match.debug_with(self.cx.body),
        );
    }
    fn log_local_conflicted(&self, local_pat: pat::Local, local: mir::Local) {
        let conflicted_local = self.matching[local_pat].matched.get().unwrap();
        debug!(
            "local conflicted: {local_pat:?}: {ty_pat:?} !! {local:?} / {conflicted_local:?}: {ty:?}",
            ty_pat = self.cx.mir_pat.locals[local_pat],
            ty = self.cx.body.local_decls[conflicted_local].ty,
        );
    }
    fn log_local_matched(&self, local_pat: pat::Local, local: mir::Local) {
        debug!(
            "local matched: {local_pat:?}: {ty_pat:?} <-> {local:?}: {ty:?}",
            ty_pat = self.cx.mir_pat.locals[local_pat],
            ty = self.cx.body.local_decls[local].ty,
        );
    }
    fn log_ty_var_matched(&self, ty_var: pat::TyVarIdx, ty: Ty<'tcx>) {
        debug!("type variable matched, {ty_var:?} <-> {ty:?}");
    }
}

impl<'tcx> Matching<'tcx> {
    /// Test if there are any empty candidates in the matches.
    fn has_empty_candidates(&self, cx: &CheckMirCtxt<'_, '_, 'tcx>) -> bool {
        self.basic_blocks
            .iter_enumerated()
            .any(|(bb, matching)| matching.has_empty_candidates(cx, bb))
            || self.locals.iter_enumerated().any(|(local, matching)| {
                matching.has_empty_candidates() && {
                    info!("Local {local:?} has no candidates");
                    true
                }
            })
        // may declare a type variable without using it.
        // || self.ty_vars.iter().any(TyVarMatches::has_empty_candidates)
    }

    #[instrument(level = "info", skip(self))]
    fn log_candidates(&self) {
        info!("pat block <-> mir candidate blocks");
        for (bb, block) in self.basic_blocks.iter_enumerated() {
            info!("pat stmt <-> mir candidate statements");
            for (index, stmt) in block.statements.iter().enumerate() {
                info!("    {bb:?}[{index}]: {:?}", stmt.candidates);
            }
        }
        info!("pat local <-> mir candidate locals");
        for (local, matches) in self.locals.iter_enumerated() {
            info!("{local:?}: {:?}", matches.candidates);
        }
        info!("pat ty metavar <-> mir candidate types");
        for (ty_var, matches) in self.ty_vars.iter_enumerated() {
            info!("{ty_var:?}: {:?}", matches.candidates);
        }
    }

    #[instrument(level = "info", skip_all)]
    fn log_matched(&self, cx: &CheckMirCtxt<'_, '_, 'tcx>) {
        for (bb, block) in self.basic_blocks.iter_enumerated() {
            for (index, stmt) in block.statements.iter().enumerate() {
                info!(
                    "{bb:?}[{index}]: {:?} <-> {:?}",
                    cx.mir_pat[bb].debug_stmt_at(index),
                    stmt.matched.get().map(|matched| matched.debug_with(cx.body))
                );
            }
        }
        for (local, matches) in self.locals.iter_enumerated() {
            info!("{local:?} <-> {:?}", matches.matched.get());
        }
        for (ty_var, matches) in self.ty_vars.iter_enumerated() {
            info!("{ty_var:?}: {:?}", matches.matched.get());
        }
    }

    fn to_matched(&self) -> Matched<'tcx> {
        let basic_blocks = self
            .basic_blocks
            .iter_enumerated()
            .map(|(bb, matching)| matching.to_matched(bb))
            .collect();
        let locals = self
            .locals
            .iter_enumerated()
            .map(|(local_pat, matching)| {
                matching
                    .get()
                    .unwrap_or_else(|| panic!("bug: local variable {local_pat:?} not matched"))
            })
            .collect();
        let ty_vars = self
            .ty_vars
            .iter_enumerated()
            .map(|(ty_var, matching)| {
                matching
                    .get()
                    .unwrap_or_else(|| panic!("bug: type variable {ty_var:?} not matched"))
            })
            .collect();
        Matched {
            basic_blocks,
            locals,
            ty_vars,
        }
    }
}

#[derive(Debug)]
struct MatchingBlock {
    statements: Vec<StatementMatches>,
    start: Cell<Option<mir::BasicBlock>>,
    end: Cell<Option<mir::BasicBlock>>,
}

impl MatchingBlock {
    fn new(num_stmts: usize) -> Self {
        Self {
            statements: core::iter::repeat_with(Default::default).take(num_stmts).collect(),
            start: Cell::new(None),
            end: Cell::new(None),
        }
    }
    /// Test if there are any empty candidates in the matches.
    fn has_empty_candidates(&self, cx: &CheckMirCtxt<'_, '_, '_>, bb: pat::BasicBlock) -> bool {
        self.statements
            .iter()
            .position(StatementMatches::has_empty_candidates)
            .inspect(|&stmt| {
                info!(
                    "Statement {bb:?}[{stmt}] has no candidates: {:?}",
                    cx.mir_pat[bb].debug_stmt_at(stmt)
                )
            })
            .is_some()
    }

    fn terminator(&self) -> &StatementMatches {
        self.statements.last().expect("bug: empty block")
    }

    fn to_matched(&self, bb_pat: pat::BasicBlock) -> MatchedBlock {
        MatchedBlock {
            statements: self
                .statements
                .iter()
                .enumerate()
                .map(|(i, stmt)| {
                    stmt.get()
                        .unwrap_or_else(|| panic!("bug: statement {bb_pat:?}[{i}] not matched"))
                })
                .collect(),
            start: self.start.get(),
            end: self.end.get(),
        }
    }
}

#[derive(Default, Debug, Clone)]
struct StatementMatches {
    matched: Cell<Option<StatementMatch>>,
    candidates: Vec<StatementMatch>,
}

impl StatementMatches {
    /// Test if there are any empty candidates in the matches.
    fn has_empty_candidates(&self) -> bool {
        if let &[m] = &self.candidates[..] {
            self.matched.set(Some(m));
        }

        self.candidates.is_empty()
    }

    fn get(&self) -> Option<StatementMatch> {
        self.matched.get()
    }

    // After `match_stmt_candidates`, all statements are supposed to be matched,
    // so we can assume that `self.matched` is `Some`.
    #[track_caller]
    fn force_get_matched(&self) -> StatementMatch {
        self.matched.get().expect("bug: statement not matched")
    }
}

#[derive(Debug)]
struct LocalMatches {
    matched: CountedMatch<mir::Local>,
    candidates: HybridBitSet<mir::Local>,
}

impl LocalMatches {
    fn new(num_locals: usize) -> Self {
        Self {
            matched: CountedMatch::default(),
            candidates: HybridBitSet::new_empty(num_locals),
        }
    }

    /// Test if there are any empty candidates in the matches.
    fn has_empty_candidates(&self) -> bool {
        self.candidates.is_empty()
    }

    fn get(&self) -> Option<mir::Local> {
        self.matched.get()
    }

    // After `match_local_candidates`, all locals are supposed to be matched,
    // so we can assume that `self.matched` is `Some`.
    #[track_caller]
    fn force_get_matched(&self) -> mir::Local {
        self.matched.get().expect("bug: local not matched")
    }
}

#[derive(Default, Debug)]
struct TyVarMatches<'tcx> {
    matched: CountedMatch<Ty<'tcx>>,
    candidates: FxIndexSet<Ty<'tcx>>,
}

impl<'tcx> TyVarMatches<'tcx> {
    fn new() -> Self {
        Self::default()
    }

    fn get(&self) -> Option<Ty<'tcx>> {
        self.matched.get()
    }

    /// Test if there are any empty candidates in the matches.
    #[allow(unused)]
    fn has_empty_candidates(&self) -> bool {
        self.candidates.is_empty()
    }

    // After `match_ty_var_candidates`, all type variables are supposed to be matched,
    // so we can assume that `self.matched` is `Some`.
    #[track_caller]
    fn force_get_matched(&self) -> Ty<'tcx> {
        self.matched.get().expect("bug: type variable not matched")
    }
}

trait IntoLocation: Copy {
    type Location;
    fn into_location(self) -> Self::Location;
}

impl IntoLocation for pat::Location {
    type Location = pat::Location;

    fn into_location(self) -> Self::Location {
        self
    }
}

impl IntoLocation for (pat::BasicBlock, usize) {
    type Location = pat::Location;

    fn into_location(self) -> Self::Location {
        pat::Location {
            block: self.0,
            statement_index: self.1,
        }
    }
}

impl IntoLocation for mir::Location {
    type Location = mir::Location;

    fn into_location(self) -> Self::Location {
        self
    }
}

impl IntoLocation for (mir::BasicBlock, usize) {
    type Location = mir::Location;

    fn into_location(self) -> Self::Location {
        mir::Location {
            block: self.0,
            statement_index: self.1,
        }
    }
}
