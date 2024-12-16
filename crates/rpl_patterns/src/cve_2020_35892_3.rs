use rpl_context::PatCtxt;
use rpl_mir::pat::{self, MirPattern};
use rpl_mir::{CheckMirCtxt, Matches};
use rustc_errors::MultiSpan;
use rustc_hir::def_id::LocalDefId;
use rustc_hir::intravisit::{self, Visitor};
use rustc_hir::{self as hir};
use rustc_middle::hir::nested_filter::All;
use rustc_middle::ty::TyCtxt;
use rustc_span::Span;

#[instrument(level = "info", skip(tcx, pcx))]
pub fn check_item(tcx: TyCtxt<'_>, pcx: PatCtxt<'_>, item_id: hir::ItemId) {
    let item = tcx.hir().item(item_id);
    // let def_id = item_id.owner_id.def_id;
    let mut check_ctxt = CheckFnCtxt::new(tcx, pcx);
    check_ctxt.visit_item(item);
}

struct CheckFnCtxt<'pcx, 'tcx> {
    tcx: TyCtxt<'tcx>,
    pcx: PatCtxt<'pcx>,
    loop_matches: Option<Matches<'tcx>>,
}

impl<'pcx, 'tcx> CheckFnCtxt<'pcx, 'tcx> {
    fn new(tcx: TyCtxt<'tcx>, pcx: PatCtxt<'pcx>) -> Self {
        let loop_matches = None;
        Self { tcx, pcx, loop_matches }
    }
}

impl<'tcx> Visitor<'tcx> for CheckFnCtxt<'_, 'tcx> {
    type NestedFilter = All;
    fn nested_visit_map(&mut self) -> Self::Map {
        self.tcx.hir()
    }

    #[instrument(level = "debug", skip_all, fields(?item.owner_id))]
    fn visit_item(&mut self, item: &'tcx hir::Item<'tcx>) -> Self::Result {
        match item.kind {
            hir::ItemKind::Trait(hir::IsAuto::No, ..) | hir::ItemKind::Impl(_) | hir::ItemKind::Fn(..) => {},
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
        span: Span,
        def_id: LocalDefId,
    ) -> Self::Result {
        if self.tcx.is_mir_available(def_id) {
            let body = self.tcx.optimized_mir(def_id);

            #[allow(irrefutable_let_patterns)]
            if let pattern = pattern_loop(self.pcx)
                && let Some(matches) = CheckMirCtxt::new(self.tcx, body, &pattern).check()
            {
                let matches = self.loop_matches.insert(matches);
                let mut multi_span = MultiSpan::from_span(span);
                matches
                    .basic_blocks
                    .iter_enumerated()
                    .flat_map(|(bb, block)| {
                        block
                            .statements
                            .iter()
                            .copied()
                            .enumerate()
                            .filter_map(move |(index, stmt)| Some((bb, index, stmt?)))
                    })
                    .for_each(|(bb, index, stmt)| {
                        let span = stmt.span_no_inline(body);
                        #[allow(rustc::untranslatable_diagnostic)]
                        multi_span.push_span_label(
                            span,
                            format!("{:?} <=> {:?}", pattern[bb].debug_stmt_at(index), stmt.debug_with(body)),
                        );
                    });
                #[allow(rustc::untranslatable_diagnostic)]
                #[allow(rustc::diagnostic_outside_of_impl)]
                self.tcx.dcx().span_note(multi_span, "MIR pattern matched");
            } else if let pattern_offset_by_len = pattern_offset_by_len(self.pcx)
                && let Some(matches) = CheckMirCtxt::new(self.tcx, body, &pattern_offset_by_len.pattern).check()
                && let Some(read) = matches[pattern_offset_by_len.read]
                && let read = read.span_no_inline(body)
                && let Some(ptr) = matches[pattern_offset_by_len.ptr]
                && let ptr = ptr.span_no_inline(body)
                && let Some(len) = matches[pattern_offset_by_len.len]
                && let len = len.span_no_inline(body)
            {
                debug!(?ptr, ?read, ?len);
                let len_local = self
                    .tcx
                    .sess
                    .source_map()
                    .span_to_snippet(len)
                    .unwrap_or_else(|_| "{expr}".to_string());
                self.tcx.dcx().emit_err(crate::errors::OffsetByOne {
                    read,
                    ptr,
                    len,
                    len_local,
                });
            }
            _ = pattern_loop(self.pcx);
        }
        intravisit::walk_fn(self, kind, decl, body_id, def_id);
    }
}

#[rpl_macros::pattern_def]
fn pattern_loop(pcx: PatCtxt<'_>) -> MirPattern<'_> {
    rpl! {
        fn $pattern(..) -> _ = mir! {
            meta!($T:ty, $SlabT:ty = |_tcx, _paramse_env, ty| ty.is_adt());

            let self: &mut $SlabT;
            let len: usize;
            let x1: usize;
            let x2: usize;
            let opt: #[lang = "Option"]<usize>;
            let discr: isize;
            let x: usize;
            let start_ref: &usize;
            let end_ref: &usize;
            let start: usize;
            let end: usize;
            let range: core::ops::range::Range<usize>;
            let mut iter: core::ops::range::Range<usize>;
            let mut iter_mut: &mut core::ops::range::Range<usize>;
            let mut base: *mut $T;
            let offset: isize;
            let elem_ptr: *mut $T;
            let cmp: bool;

            len = copy (*self).len;
            range = core::ops::range::Range { start: const 0_usize, end: move len };
            iter = move range;
            loop {
                iter_mut = &mut iter;
                start_ref = &(*iter_mut).start;
                start = copy *start_ref;
                end_ref = &(*iter_mut).end;
                end = copy *end_ref;
                cmp = Lt(move start, move end);
                switchInt(move cmp) {
                    false => opt = #[lang = "None"],
                    _ => {
                        x1 = copy (*iter_mut).start;
                        x2 = core::iter::range::Step::forward_unchecked(copy x1, const 1_usize);
                        (*iter_mut).start = move x2;
                        opt = #[lang = "Some"](copy x1);
                    }
                }
                discr = discriminant(opt);
                switchInt(move discr) {
                    0_isize => break,
                    1_isize => {
                        x = copy (opt as Some).0;
                        base = copy (*self).mem;
                        offset = copy x as isize (IntToInt);
                        elem_ptr = Offset(copy base, copy offset);
                        _ = core::ptr::drop_in_place(copy elem_ptr);
                    }
                }
            }
        }
    }
    pattern
}

struct PatternOffsetByLen<'pcx> {
    pattern: MirPattern<'pcx>,
    len: pat::Location,
    ptr: pat::Location,
    read: pat::Location,
}

#[rpl_macros::pattern_def]
fn pattern_offset_by_len(pcx: PatCtxt<'_>) -> PatternOffsetByLen<'_> {
    rpl! {
        fn $pattern(..) -> _ = mir! {
            meta!($T:ty, $SlabT:ty = |_tcx, _paramse_env, ty| ty.is_adt());
            let self: &mut $SlabT;
            let len: usize = copy (*self).len;
            let len_isize: isize = move len as isize (IntToInt);
            let base: *mut $T = copy (*self).mem;
            let ptr_mut: *mut $T = Offset(copy base, copy len_isize);
            let ptr: *const $T = copy ptr_mut as *const $T (PtrToPtr);
            let elem: $T = copy (*ptr);
        }
    }

    PatternOffsetByLen {
        pattern,
        len: len_stmt,
        ptr: ptr_mut_stmt,
        read: elem_stmt,
    }
}
