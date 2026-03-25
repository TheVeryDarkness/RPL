use std::ops::Index;

use rpl_constraints::Const;
use rpl_constraints::attributes::ExtraSpan;
use rpl_context::pat::{self, MatchedMetaVars, Spanned};
use rustc_data_structures::sorted_map::SortedMap;
use rustc_hir::def_id::LocalDefId;
use rustc_index::IndexVec;
use rustc_middle::{mir, ty};
use rustc_span::{Span, Symbol};

use crate::match2::with_call_stack::WithCallStack;
use crate::matches::artifact::NormalizedSpanned;

pub(crate) type StatementMatch = crate::matches::StatementMatch;

#[derive(Debug)]
pub struct Matched<'tcx> {
    pub basic_blocks: IndexVec<pat::BasicBlock, MatchedBlock>,
    pub locals: IndexVec<pat::Local, WithCallStack<mir::Local>>,
    pub ty_vars: IndexVec<pat::TyVarIdx, ty::Ty<'tcx>>,
    pub const_vars: IndexVec<pat::ConstVarIdx, Const<'tcx>>,
    pub place_vars: IndexVec<pat::PlaceVarIdx, WithCallStack<mir::PlaceRef<'tcx>>>,
}

impl Matched<'_> {
    pub(crate) fn log_matched(&self) {
        use tracing::debug as info;
        info!("pat block <-> mir candidate blocks");
        for (bb, block) in self.basic_blocks.iter_enumerated() {
            info!("pat stmt <-> mir candidate statements");
            for (index, stmt) in block.statements.iter().enumerate() {
                info!("    {bb:?}[{index}]: {:?}", stmt);
            }
        }
        info!("pat local <-> mir candidate locals");
        for (local, matches) in self.locals.iter_enumerated() {
            info!("{local:?}: {:?}", matches);
        }
        info!("pat ty metavar <-> mir candidate types");
        for (ty_var, matches) in self.ty_vars.iter_enumerated() {
            info!("{ty_var:?}: {:?}", matches);
        }
        info!("pat const metavar <-> mir candidate constants");
        for (const_var, matches) in self.const_vars.iter_enumerated() {
            info!("{const_var:?}: {:?}", matches);
        }
        info!("pat place metavar <-> mir candidate places");
        for (place_var, matches) in self.place_vars.iter_enumerated() {
            info!("{place_var:?}: {:?}", matches);
        }
    }
}

impl<'tcx> MatchedMetaVars<'tcx> for NormalizedMatched<'tcx> {
    fn type_meta_var(&self, idx: pat::TyVarIdx) -> ty::Ty<'tcx> {
        self.ty_vars[idx]
    }
    fn const_meta_var(&self, idx: pat::ConstVarIdx) -> Const<'tcx> {
        self.const_vars[idx]
    }
    fn place_meta_var(&self, idx: pat::PlaceVarIdx, _: LocalDefId) -> (LocalDefId, mir::PlaceRef<'tcx>) {
        self.place_vars[idx]
    }
}
impl<'a, 'tcx, Cx: pat::MirGraphs<'tcx>> pat::Matched<'a, 'tcx, &'a Cx> for NormalizedMatched<'tcx> {
    fn bottom_span(&self, cx: &Cx) -> Span {
        cx.get_fn(self.bottom).1.span
    }
    fn bottom_name(&self, cx: &Cx) -> Option<Symbol> {
        cx.get_fn(self.bottom).0
    }
    fn span(&self, fns: &Cx, name: &str) -> Span {
        let labels = &self.extra;
        let i = Symbol::intern(name);
        let (id, span) = &labels[&i];
        let (_name, body, decl) = fns.get_fn(*id);
        span.span(body, decl)
    }
}

/// A normalized version of [`Matched`].
///
/// This to [`Matched`] is analogous to [`crate::matches::artifact::NormalizedMatched`] to
/// [`crate::matches::Matched`]. See [`crate::matches::artifact::NormalizedMatched`] for what
/// "normalization" means in this context.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct NormalizedMatched<'tcx> {
    bottom: LocalDefId,

    ty_vars: IndexVec<pat::TyVarIdx, ty::Ty<'tcx>>,
    const_vars: IndexVec<pat::ConstVarIdx, Const<'tcx>>,
    place_vars: IndexVec<pat::PlaceVarIdx, (LocalDefId, mir::PlaceRef<'tcx>)>,

    /// Labels and attributes. Sorted by label.
    extra: SortedMap<Symbol, (LocalDefId, NormalizedSpanned)>,
}

impl<'tcx> NormalizedMatched<'tcx> {
    fn map_spanned(
        label: Symbol,
        spanned: &Spanned,
        bottom: LocalDefId,
        matched: &Matched<'tcx>,
    ) -> (Symbol, (LocalDefId, NormalizedSpanned)) {
        match spanned {
            Spanned::Location(location) => {
                let matched = &matched[*location];
                let (bottom, _) = matched.bottom();
                let location = matched.value();
                (label, (bottom, NormalizedSpanned::Location(location)))
            },
            Spanned::Local(local) => {
                let matched = &matched[*local];
                let (bottom, _) = matched.bottom();
                let local = matched.value();
                (label, (bottom, NormalizedSpanned::Local(local)))
            },
            // FIXME: these two should really record the bottom function id
            Spanned::Body => (label, (bottom, NormalizedSpanned::Body)),
            Spanned::Output => (label, (bottom, NormalizedSpanned::Output)),
        }
    }
    pub(crate) fn new(
        fn_id: LocalDefId,
        matched: Matched<'tcx>,
        label_map: &pat::LabelMap,
        extra_spans: &ExtraSpan<'tcx>,
    ) -> Self {
        let ty_vars = matched.ty_vars.clone();
        let const_vars = matched.const_vars.clone();
        let place_vars = matched
            .place_vars
            .iter()
            .map(|matched| (matched.bottom().0, matched.value()))
            .collect();
        let labels: SortedMap<_, (LocalDefId, NormalizedSpanned)> = label_map
            .iter()
            .map(|(label, spanned)| Self::map_spanned(*label, spanned, fn_id, &matched))
            .chain(
                extra_spans
                    .iter()
                    .map(|(label, span)| (*label, (fn_id, NormalizedSpanned::Span(span.span())))),
            )
            .collect();

        NormalizedMatched {
            bottom: fn_id,
            ty_vars,
            const_vars,
            place_vars,
            extra: labels,
        }
    }
    pub fn bottom(&self) -> LocalDefId {
        self.bottom
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct MatchedBlock {
    pub statements: Vec<WithCallStack<StatementMatch>>,
}

impl Index<pat::BasicBlock> for Matched<'_> {
    type Output = MatchedBlock;

    fn index(&self, bb: pat::BasicBlock) -> &Self::Output {
        &self.basic_blocks[bb]
    }
}

impl Index<pat::Location> for Matched<'_> {
    type Output = WithCallStack<StatementMatch>;

    fn index(&self, stmt: pat::Location) -> &Self::Output {
        &self.basic_blocks[stmt.block].statements[stmt.statement_index]
    }
}

impl Index<pat::Local> for Matched<'_> {
    type Output = WithCallStack<mir::Local>;

    fn index(&self, local: pat::Local) -> &Self::Output {
        &self.locals[local]
    }
}
