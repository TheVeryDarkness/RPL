use std::cell::{Cell, RefCell};
use std::ops::{Deref, Index};

use rpl_constraints::Const;
use rpl_context::{PatCtxt, pat};
use rustc_hash::FxHashMap;
use rustc_hir::FnDecl;
use rustc_hir::def_id::LocalDefId;
use rustc_index::IndexVec;
use rustc_middle::ty::{TyCtxt, TypingEnv};
use rustc_middle::{mir, ty};
use rustc_span::{Ident, Symbol};

use crate::AdtMatch;
use crate::graph::{MirControlFlowGraph, MirDataDepGraph, PatControlFlowGraph, PatDataDepGraph};
use crate::matches::Matched;
use crate::statement::MatchStatement;
use crate::ty::MatchTy;

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
    /// Copied from [`crate::place::MatchPlaceCtxt`].
    places: IndexVec<pat::PlaceVarIdx, pat::Ty<'pcx>>,
    fns: &'a [MirGraph<'tcx>],
}

impl<'a, 'pcx, 'tcx> MatchCtxt2<'a, 'pcx, 'tcx> {
    fn new_checking(&self, fn_pat: &pat::FnPatternBody<'pcx>, body: &mir::Body<'tcx>) -> Matching<'tcx> {
        let cx = self;
        let num_blocks = fn_pat.basic_blocks.len();
        let num_locals = fn_pat.locals.len();
        let mir_statements = IndexVec::from_fn_n(
            |bb| MirStatementBackMatches::from_elem_n(MatchingCell::new(), body[bb].statements.len()),
            body.basic_blocks.len(),
        );
        Matching {
            basic_blocks: IndexVec::from_fn_n(
                |bb_pat| {
                    let mut num_stmt_pats = fn_pat[bb_pat].num_statements_and_terminator();
                    // We don't need to match the end of the pattern, because it is only a marker and has no
                    // corresponding terminator.
                    if fn_pat[bb_pat].has_pat_end() {
                        num_stmt_pats -= 1;
                    }
                    MatchingBlock::from_elem_n(MatchingCell::new(), num_stmt_pats)
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
    fn find_1_component_matches(&self) {
        if let Some(fn_pat) = self.fn_pat.body {
            for fn_graph in self.fns {
                let mut matching_1 = Vec::new();
                // Find all possible matches of 1-component in pattern graph to MIR graph.
                for (bb_pat, block_pat) in fn_pat.basic_blocks.iter_enumerated() {
                    for (stmt_pat_idx, stmt_pat) in block_pat.statements.iter().enumerate() {
                        let loc_pat = pat::Location {
                            block: bb_pat,
                            statement_index: stmt_pat_idx,
                        };
                        for (bb, block) in fn_graph.body.basic_blocks.iter_enumerated() {
                            for (stmt_idx, stmt) in block.statements.iter().enumerate() {
                                let loc = mir::Location {
                                    block: bb,
                                    statement_index: stmt_idx,
                                };
                                let matching = self.new_checking(fn_pat, fn_graph.body);
                                let cx = MatchCtxt2Once {
                                    cx: self,
                                    body: fn_graph.body,
                                    self_ty: fn_graph.self_ty,
                                    typing_env: fn_graph.typing_env,
                                    fn_pat,
                                    pat: self.pat,
                                    matching,
                                };
                                if cx.match_statement(loc_pat, loc, stmt_pat, stmt) {
                                    matching_1.push(cx.matching);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

struct MatchCtxt2Once<'a, 'pcx, 'tcx> {
    cx: &'a MatchCtxt2<'a, 'pcx, 'tcx>,
    body: &'a mir::Body<'tcx>,
    self_ty: Option<ty::Ty<'tcx>>,
    typing_env: TypingEnv<'tcx>,
    fn_pat: &'a pat::FnPatternBody<'pcx>,
    pat: &'pcx pat::RustItems<'pcx>,
    matching: Matching<'tcx>,
}

impl<'a, 'pcx, 'tcx> MatchStatement<'pcx, 'tcx> for MatchCtxt2Once<'a, 'pcx, 'tcx> {
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

    fn match_local(&self, pat: pat::Local, local: mir::Local) -> bool {
        self.matching[pat].try_set(local)
    }
    fn match_place_var(&self, pat: pat::PlaceVarIdx, place: mir::PlaceRef<'tcx>) -> bool {
        self.matching[pat].try_set(place)
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

    fn match_ty_var(&self, ty_var: pat::TyVar, ty: ty::Ty<'tcx>) -> bool {
        self.matching[ty_var.idx].try_set(ty)
    }
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
    fn match_mir_const_var(&self, const_var: pat::ConstVar<'pcx>, konst: mir::Const<'tcx>) -> bool {
        self.match_ty(const_var.ty, konst.ty()) && self.matching[const_var.idx].try_set(konst.into())
    }
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
}

impl<T: PartialEq + Copy> MatchingCell<T> for Cell<Option<T>> {
    #[must_use]
    fn try_set(&self, new: T) -> bool {
        if self.get().as_ref().is_none_or(|old| *old == new) {
            self.set(Some(new));
            true
        } else {
            false
        }
    }
    fn new() -> Self {
        Cell::new(None)
    }
}

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

type MatchingBlock = IndexVec<usize, StatementMatches>;
type StatementMatches = Cell<Option<mir::Location>>;
type LocalMatches = Cell<Option<mir::Local>>;
type TyVarMatches<'tcx> = Cell<Option<ty::Ty<'tcx>>>;
type ConstVarMatches<'tcx> = Cell<Option<Const<'tcx>>>;
type PlaceVarMatches<'tcx> = Cell<Option<mir::PlaceRef<'tcx>>>;
type MirStatementBackMatches = IndexVec<usize, Cell<Option<pat::Location>>>;
type MirStatementBackMatch = Cell<Option<pat::Location>>;
type AdtMatches<'tcx> = RefCell<Option<AdtMatch<'tcx>>>;

/// See its counterpart in [`crate::matches`].
// FIXME: use a sparse representation to save memory.
#[derive(Debug)]
struct Matching<'tcx> {
    basic_blocks: IndexVec<pat::BasicBlock, MatchingBlock>,
    locals: IndexVec<pat::Local, LocalMatches>,
    ty_vars: IndexVec<pat::TyVarIdx, TyVarMatches<'tcx>>,
    const_vars: IndexVec<pat::ConstVarIdx, ConstVarMatches<'tcx>>,
    place_vars: IndexVec<pat::PlaceVarIdx, PlaceVarMatches<'tcx>>,
    /// Track which pattern statement the statement is matched to,
    /// so that one statement in MIR can be matched to at most one statement in pattern.
    mir_statements: IndexVec<mir::BasicBlock, MirStatementBackMatches>,
    /// See [`crate::adt::AdtMatch`].
    adt_matches: FxHashMap<Symbol, AdtMatches<'tcx>>,
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

// impl Index<pat::BasicBlock> for Matching<'_> {
//     type Output = MatchingBlock;

//     fn index(&self, bb: pat::BasicBlock) -> &Self::Output {
//         &self.basic_blocks[bb]
//     }
// }

// impl Index<pat::Location> for Matching<'_> {
//     type Output = StatementMatches;

//     fn index(&self, stmt: pat::Location) -> &Self::Output {
//         &self.basic_blocks[stmt.block][stmt.statement_index]
//     }
// }

// impl Index<pat::Local> for Matching<'_> {
//     type Output = LocalMatches;

//     fn index(&self, local: pat::Local) -> &Self::Output {
//         &self.locals[local]
//     }
// }

// impl<'tcx> Index<pat::TyVarIdx> for Matching<'tcx> {
//     type Output = TyVarMatches<'tcx>;

//     fn index(&self, ty_var: pat::TyVarIdx) -> &Self::Output {
//         &self.ty_vars[ty_var]
//     }
// }

// impl<'tcx> Index<pat::ConstVarIdx> for Matching<'tcx> {
//     type Output = ConstVarMatches<'tcx>;

//     fn index(&self, const_var: pat::ConstVarIdx) -> &Self::Output {
//         &self.const_vars[const_var]
//     }
// }

// impl<'tcx> Index<pat::PlaceVarIdx> for Matching<'tcx> {
//     type Output = PlaceVarMatches<'tcx>;

//     fn index(&self, place_var: pat::PlaceVarIdx) -> &Self::Output {
//         &self.place_vars[place_var]
//     }
// }

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
pub fn check2<'a, 'pcx, 'tcx: 'a>(
    tcx: TyCtxt<'tcx>,
    pcx: PatCtxt<'pcx>,
    pat: &'pcx pat::RustItems<'pcx>,
    pat_name: Symbol,
    pat_cfg: &'a PatControlFlowGraph,
    pat_ddg: &'a PatDataDepGraph,
    fn_pat: &'a pat::FnPattern<'pcx>,
    fns: &'a [MirGraph<'tcx>],
) -> Vec<(&'a MirGraph<'tcx>, Matched<'tcx>)> {
    let places = pat.meta.place_vars.iter().map(|var| var.ty).collect();
    let cx = MatchCtxt2 {
        tcx,
        pcx,
        pat,
        pat_name,
        fn_pat,
        pat_cfg,
        pat_ddg,
        places,
        fns,
    };
    cx.find_1_component_matches();
    todo!()
}
