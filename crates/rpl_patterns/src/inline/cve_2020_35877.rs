use crate::lints::{DEREF_UNCHECKED_PTR_OFFSET, UNCHECKED_POINTER_OFFSET};
use rpl_context::PatCtxt;
use rpl_mir::pat::Location;
use rpl_mir::{CheckMirCtxt, Matched, StatementMatch, pat};
use rustc_hir::def_id::LocalDefId;
use rustc_hir::intravisit::{self, Visitor};
use rustc_hir::{self as hir};
use rustc_middle::bug;
use rustc_middle::hir::nested_filter::All;
use rustc_middle::mir::Body;
use rustc_middle::ty::TyCtxt;
use rustc_span::{Span, Symbol};
use std::collections::BTreeSet;
use std::ops::Not;

#[instrument(level = "info", skip_all)]
pub fn check_item(tcx: TyCtxt<'_>, pcx: PatCtxt<'_>, item_id: hir::ItemId) {
    let item = tcx.hir().item(item_id);
    // let def_id = item_id.owner_id.def_id;
    let mut check_ctxt = CheckFnCtxt::new(tcx, pcx);
    check_ctxt.visit_item(item);
}

struct CheckFnCtxt<'pcx, 'tcx> {
    tcx: TyCtxt<'tcx>,
    pcx: PatCtxt<'pcx>,
}

impl<'pcx, 'tcx> CheckFnCtxt<'pcx, 'tcx> {
    fn new(tcx: TyCtxt<'tcx>, pcx: PatCtxt<'pcx>) -> Self {
        Self { tcx, pcx }
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
        // let attrs: Vec<_> = self
        //     .tcx
        //     .get_attrs_by_path(def_id.to_def_id(), &[Symbol::intern("rpl"), Symbol::intern("check")])
        //     .collect();
        // info!("attrs: {:?}", attrs);
        // if attrs.is_empty() {
        //     return;
        // }

        if kind.header().is_none_or(|header| header.is_unsafe().not()) && self.tcx.is_mir_available(def_id) {
            let body = self.tcx.optimized_mir(def_id);

            // There are two patterns for checked offsets, one for the specific case and one for the general
            // case

            let pattern = pattern_unchecked_ptr_offset(self.pcx);
            for matches in CheckMirCtxt::new(self.tcx, self.pcx, body, pattern.pattern, pattern.fn_pat).check() {
                let ptr = matches[pattern.ptr].span_no_inline(body);
                let offset = matches[pattern.offset].span_no_inline(body);
                let reference = matches[pattern.reference].span_no_inline(body);
                debug!(?ptr, ?offset, ?reference);
                self.tcx.emit_node_span_lint(
                    DEREF_UNCHECKED_PTR_OFFSET,
                    self.tcx.local_def_id_to_hir_id(def_id),
                    reference,
                    crate::errors::DerefUncheckedPtrOffset { reference, ptr, offset },
                );
            }

            // The pattern means: there exists a pointer `ptr` and an offset `offset` such that `ptr` is
            // offset by `offset`, but no check is performed on the offset.
            //
            // This is a more general pattern than the previous one, as it does not assume the pointer is offset
            // inside a loop.
            //
            // However, it may produce false positives, as the offset and the length may be constrained by a
            // compilation-time constant.
            fn collect_matched(
                matched: &Matched<'_>,
                ptr: Location,
                offset: Location,
                body: &Body<'_>,
            ) -> (StatementMatch, StatementMatch) {
                let ptr_ = matched[ptr];
                let offset_ = matched[offset];
                let span_ptr = ptr_.span_no_inline(body);
                let span_offset = offset_.span_no_inline(body);
                trace!(ptr = ?ptr_, offset = ?offset_, pattern.ptr = ?ptr, pattern.offset = ?offset, ?span_ptr, ?span_offset, "checked offset found");
                (ptr_, offset_)
            }
            fn collect_checked<'m, 'tcx, 'pcx>(
                positive: (&'m [Matched<'tcx>], PatternUncheckedPtrOffsetGeneral<'pcx>),
                negative_general: &[(&'m [Matched<'tcx>], PatternUncheckedPtrOffsetGeneral<'pcx>)],
                negative_const: &[(&'m [Matched<'tcx>], PatternUncheckedPtrOffsetConst<'pcx>)],
                def_id: LocalDefId,
                body: &Body<'tcx>,
                tcx: TyCtxt<'tcx>,
            ) {
                let locations: BTreeSet<_> = negative_general
                    .into_iter()
                    .flat_map(|(matches, pattern)| {
                        matches
                            .iter()
                            .map(move |matches| collect_matched(matches, pattern.ptr, pattern.offset, body))
                    })
                    .chain(negative_const.into_iter().flat_map(|(matches, pattern)| {
                        matches.iter().filter_map(move |matches| {
                            let const_size = matches[pattern.const_size.idx];
                            let const_size = const_size
                                .try_to_scalar_int()
                                .unwrap_or_else(|| bug!("{const_size} is not a scalar int"))
                                .to_target_usize(tcx);
                            let const_offset = matches[pattern.const_offset.idx];
                            let const_offset = const_offset
                                .try_to_scalar_int()
                                .unwrap_or_else(|| bug!("{const_offset} is not a scalar int"))
                                .to_target_usize(tcx);
                            if const_offset < const_size {
                                Some(collect_matched(matches, pattern.ptr, pattern.offset, body))
                            } else {
                                // The offset is out of bounds, so this is not a valid negative case
                                None
                            }
                        })
                    }))
                    .collect();

                let (matches_1, pattern_1) = positive;
                for matches in matches_1 {
                    let ptr = matches[pattern_1.ptr];
                    let offset = matches[pattern_1.offset];
                    if locations.contains(&(ptr, offset)) {
                        // The offset is checked, so don't emit an error
                        continue;
                    }
                    let span_ptr = ptr.span_no_inline(body);
                    let span_offset = offset.span_no_inline(body);
                    debug!(?ptr, ?offset, ?pattern_1.ptr, ?pattern_1.offset, ?span_ptr, ?span_offset, "unchecked offset found");
                    let ptr = span_ptr;
                    let offset = span_offset;
                    tcx.emit_node_span_lint(
                        UNCHECKED_POINTER_OFFSET,
                        tcx.local_def_id_to_hir_id(def_id),
                        offset,
                        crate::errors::UncheckedPtrOffset { ptr, offset },
                    );
                }
            }
            {
                let pattern_1 = pattern_unchecked_ptr_offset_(self.pcx);
                let matches_1 =
                    CheckMirCtxt::new(self.tcx, self.pcx, body, pattern_1.pattern, pattern_1.fn_pat).check();
                let pattern_2 = pattern_checked_ptr_offset_lt(self.pcx);
                let matches_2 =
                    CheckMirCtxt::new(self.tcx, self.pcx, body, pattern_2.pattern, pattern_2.fn_pat).check();
                let pattern_3 = pattern_checked_ptr_offset_le(self.pcx);
                let matches_3 =
                    CheckMirCtxt::new(self.tcx, self.pcx, body, pattern_3.pattern, pattern_3.fn_pat).check();
                let pattern_4 = pattern_checked_ptr_offset_gt(self.pcx);
                let matches_4 =
                    CheckMirCtxt::new(self.tcx, self.pcx, body, pattern_4.pattern, pattern_4.fn_pat).check();
                let pattern_5 = pattern_checked_ptr_offset_ge(self.pcx);
                let matches_5 =
                    CheckMirCtxt::new(self.tcx, self.pcx, body, pattern_5.pattern, pattern_5.fn_pat).check();
                let pattern_6 = pattern_checked_ptr_offset_rem(self.pcx);
                let matches_6 =
                    CheckMirCtxt::new(self.tcx, self.pcx, body, pattern_6.pattern, pattern_6.fn_pat).check();
                let pattern_7 = pattern_checked_ptr_offset_slice_len(self.pcx);
                let matches_7 =
                    CheckMirCtxt::new(self.tcx, self.pcx, body, pattern_7.pattern, pattern_7.fn_pat).check();
                let pattern_8 = pattern_checked_ptr_offset_const(self.pcx);
                let matches_8 =
                    CheckMirCtxt::new(self.tcx, self.pcx, body, pattern_8.pattern, pattern_8.fn_pat).check();

                collect_checked(
                    (&matches_1, pattern_1),
                    &[
                        (&matches_2, pattern_2),
                        (&matches_3, pattern_3),
                        (&matches_4, pattern_4),
                        (&matches_5, pattern_5),
                        (&matches_6, pattern_6),
                        (&matches_7, pattern_7),
                    ],
                    &[(&matches_8, pattern_8)],
                    def_id,
                    body,
                    self.tcx,
                );
            }
            {
                let pattern_1 = pattern_unchecked_mut_ptr_offset_(self.pcx);
                let matches_1 =
                    CheckMirCtxt::new(self.tcx, self.pcx, body, pattern_1.pattern, pattern_1.fn_pat).check();
                let pattern_2 = pattern_checked_mut_ptr_offset_lt(self.pcx);
                let matches_2 =
                    CheckMirCtxt::new(self.tcx, self.pcx, body, pattern_2.pattern, pattern_2.fn_pat).check();
                let pattern_3 = pattern_checked_mut_ptr_offset_le(self.pcx);
                let matches_3 =
                    CheckMirCtxt::new(self.tcx, self.pcx, body, pattern_3.pattern, pattern_3.fn_pat).check();
                let pattern_4 = pattern_checked_mut_ptr_offset_gt(self.pcx);
                let matches_4 =
                    CheckMirCtxt::new(self.tcx, self.pcx, body, pattern_4.pattern, pattern_4.fn_pat).check();
                let pattern_5 = pattern_checked_mut_ptr_offset_ge(self.pcx);
                let matches_5 =
                    CheckMirCtxt::new(self.tcx, self.pcx, body, pattern_5.pattern, pattern_5.fn_pat).check();
                let pattern_6 = pattern_checked_mut_ptr_offset_rem(self.pcx);
                let matches_6 =
                    CheckMirCtxt::new(self.tcx, self.pcx, body, pattern_6.pattern, pattern_6.fn_pat).check();
                let pattern_7 = pattern_checked_mut_ptr_offset_slice_len(self.pcx);
                let matches_7 =
                    CheckMirCtxt::new(self.tcx, self.pcx, body, pattern_7.pattern, pattern_7.fn_pat).check();
                let pattern_8 = pattern_checked_mut_ptr_offset_const(self.pcx);
                let matches_8 =
                    CheckMirCtxt::new(self.tcx, self.pcx, body, pattern_8.pattern, pattern_8.fn_pat).check();

                collect_checked(
                    (&matches_1, pattern_1),
                    &[
                        (&matches_2, pattern_2),
                        (&matches_3, pattern_3),
                        (&matches_4, pattern_4),
                        (&matches_5, pattern_5),
                        (&matches_6, pattern_6),
                        (&matches_7, pattern_7),
                    ],
                    &[(&matches_8, pattern_8)],
                    def_id,
                    body,
                    self.tcx,
                );
            }
        }
        intravisit::walk_fn(self, kind, decl, body_id, def_id);
    }
}

struct PatternUncheckedPtrOffset<'pcx> {
    pattern: &'pcx pat::Pattern<'pcx>,
    fn_pat: &'pcx pat::Fn<'pcx>,
    ptr: pat::Location,
    offset: pat::Location,
    reference: pat::Location,
}

#[rpl_macros::pattern_def]
fn pattern_unchecked_ptr_offset(pcx: PatCtxt<'_>) -> PatternUncheckedPtrOffset<'_> {
    let ptr;
    let offset;
    let mut reference = Location::uninitialized();
    let pattern = rpl! {
        #[meta($T:ty)]
        fn $pattern(..) -> _ = mir! {
            #[export(offset)]
            let $offset: usize = _; // _?0 <-> _2 ?bb0[0] <-> _2
            let $offset_1: usize = copy $offset; // _?1 <-> _3 ?bb0[1] <-> bb0[0]
            #[export(ptr)]
            let $ptr_1: *const $T = _; // _?2 <-> _4 ?bb0[2] <-> bb3[0]
            let $offset_2: usize; // _?3 <-> _13
            let $flag: bool; // _?4 <-> _12
            let $ptr_3: *const $T; // _?5 <-> _14
            let $ptr_4: *const $T; // _?6 <-> _15
            let $reference: &$T; // _?7 <-> _0
            loop { // ?bb0[3] <-> bb3[1]
                $offset_2 = copy $offset_1; // ?bb1[0] <-> bb4[0]
                $flag = Gt(move $offset_2, const 0usize); // ?bb1[1] <-> bb4[0]
                switchInt(move $flag) { // ?bb1[2]
                    0usize => {
                        #[export(reference)]
                        $reference = &(*$ptr_1); // ?bb4[0]
                        break; // ?bb4[1]
                    }
                    _ => {
                        $offset_1 = Sub(copy $offset_1, const 1usize); // ?bb5[0] <-> bb5[0]
                        $ptr_4 = copy $ptr_1; // ?bb5[1] <-> bb5[1]
                        $ptr_3 = Offset(copy $ptr_4, _); // ?bb5[2] <-> bb5[3]
                        // FIXME: we can't distinguish between the two assignments to `$ptr_1`, so we get two errors
                        $ptr_1 = move $ptr_3; // ?bb5[3] <-> bb5[4]
                        // FIXME: without this, a basic block, where there is only one goto statement, is generated
                        continue; // ?bb5[4] <-> bb5[5]
                    }
                }
            }
        }
    };
    let fn_pat = pattern.fns.get_fn_pat(Symbol::intern("pattern")).unwrap();

    PatternUncheckedPtrOffset {
        pattern,
        fn_pat,
        ptr,
        offset,
        reference,
    }
}

struct PatternUncheckedPtrOffsetGeneral<'pcx> {
    pattern: &'pcx pat::Pattern<'pcx>,
    fn_pat: &'pcx pat::Fn<'pcx>,
    ptr: pat::Location,
    offset: pat::Location,
}

macro_rules! template {
    ($name:ident -> $ret:ident { $($fields:ident),* $(,)? } {$($inner:tt)*}) => {
        #[rpl_macros::pattern_def]
        fn $name(pcx: PatCtxt<'_>) -> $ret<'_> {
            $(
                let $fields;
            )*
            let pattern = rpl! {
                $($inner)*
            };
            let fn_pat = pattern.fns.get_fn_pat(Symbol::intern("pattern")).unwrap();

            $ret { pattern, fn_pat, $($fields),* }
        }
    };
}

// #[rpl_macros::pattern_def]
// fn pattern_unchecked_ptr_offset_(pcx: PatCtxt<'_>) -> PatternUncheckedPtrOffsetGeneral<'_> {
//     let ptr;
//     let offset;
//     let pattern = rpl! {
//         #[meta($T:ty)]
//         fn $pattern(..) -> _ = mir! {
//             #[export(ptr)]
//             let $ptr: *const $T = _;
//             #[export(offset)]
//             let $ptr_1: *const $T = Offset(copy $ptr, _);
//         }
//     };
//     let fn_pat = pattern.fns.get_fn_pat(Symbol::intern("pattern")).unwrap();

//     PatternUncheckedPtrOffsetGeneral {
//         pattern,
//         fn_pat,
//         ptr,
//         offset,
//     }
// }

// macro_rules! pattern_checked_ptr_offset {
//     ($name:ident, $($cmp_expr:tt)*) => {
//         #[rpl_macros::pattern_def]
//         fn $name(pcx: PatCtxt<'_>) -> PatternUncheckedPtrOffsetGeneral<'_> {
//             let ptr;
//             let offset;
//             let pattern = rpl! {
//                 #[meta($T:ty, $U:ty)]
//                 fn $pattern(..) -> _ = mir! {
//                     let $index: $U = _;
//                     #[export(ptr)]
//                     let $ptr: *const $T = _;
//                     let $cmp: bool = $($cmp_expr)*;
//                     #[export(offset)]
//                     let $ptr_1: *const $T = Offset(copy $ptr, _);
//                 }
//             };
//             let fn_pat = pattern.fns.get_fn_pat(Symbol::intern("pattern")).unwrap();

//             PatternUncheckedPtrOffsetGeneral {
//                 pattern,
//                 fn_pat,
//                 ptr,
//                 offset,
//             }
//         }
//     };
// }

template! {
    pattern_unchecked_ptr_offset_ -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
        #[meta($T:ty)]
        fn $pattern(..) -> _ = mir! {
            #[export(ptr)]
            let $ptr: *const $T = _;
            #[export(offset)]
            let $ptr_1: *const $T = Offset(copy $ptr, _);
        }
    }
}

template! {
    pattern_checked_ptr_offset_lt -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
        #[meta($T:ty, $U:ty)]
        fn $pattern(..) -> _ = mir! {
            let $index: $U = _;
            #[export(ptr)]
            let $ptr: *const $T = _;
            let $cmp: bool = Lt(copy $index, _);
            #[export(offset)]
            let $ptr_1: *const $T = Offset(copy $ptr, _);
        }
    }
}

template! {
    pattern_checked_ptr_offset_le -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
        #[meta($T:ty, $U:ty)]
        fn $pattern(..) -> _ = mir! {
            let $index: $U = _;
            #[export(ptr)]
            let $ptr: *const $T = _;
            let $cmp: bool = Le(copy $index, _);
            #[export(offset)]
            let $ptr_1: *const $T = Offset(copy $ptr, _);
        }
    }
}

template! {
    pattern_checked_ptr_offset_gt -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
        #[meta($T:ty, $U:ty)]
        fn $pattern(..) -> _ = mir! {
            let $index: $U = _;
            #[export(ptr)]
            let $ptr: *const $T = _;
            let $cmp: bool = Gt(_, copy $index);
            #[export(offset)]
            let $ptr_1: *const $T = Offset(copy $ptr, _);
        }
    }
}

template! {
    pattern_checked_ptr_offset_ge -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
        #[meta($T:ty, $U:ty)]
        fn $pattern(..) -> _ = mir! {
            let $index: $U = _;
            #[export(ptr)]
            let $ptr: *const $T = _;
            let $cmp: bool = Ge(_, copy $index);
            #[export(offset)]
            let $ptr_1: *const $T = Offset(copy $ptr, _);
        }
    }
}

template! {
    pattern_checked_ptr_offset_rem -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
        #[meta($T:ty, $U:ty)]
        fn $pattern(..) -> _ = mir! {
            #[export(ptr)]
            let $ptr: *const $T = _;
            let $index: $U = Rem(_, _);
            #[export(offset)]
            let $ptr_1: *const $T = Offset(copy $ptr, copy $index);
        }
    }
}

template! {
    pattern_checked_ptr_offset_slice_len -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
        #[meta($T:ty, $U:ty)]
        fn $pattern(..) -> _ = mir! {
            let $slice_ptr: *const [$T] = _; // _5
            #[export(ptr)]
            let $ptr: *const $T = move $slice_ptr as *const $T (PtrToPtr); // _3
            let $index: $U = PtrMetadata(_); // _4
            #[export(offset)]
            let $ptr_1: *const $T = Offset(copy $ptr, _); // _0
        }
    }
}

template! {
    pattern_unchecked_mut_ptr_offset_ -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
        #[meta($T:ty)]
        fn $pattern(..) -> _ = mir! {
            #[export(ptr)]
            let $ptr: *mut $T = _;
            #[export(offset)]
            let $ptr_1: *mut $T = Offset(copy $ptr, _);
        }
    }
}

template! {
    pattern_checked_mut_ptr_offset_lt -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
        #[meta($T:ty, $U:ty)]
        fn $pattern(..) -> _ = mir! {
            let $index: $U = _;
            #[export(ptr)]
            let $ptr: *mut $T = _;
            let $cmp: bool = Lt(copy $index, _);
            #[export(offset)]
            let $ptr_1: *mut $T = Offset(copy $ptr, _);
        }
    }
}

template! {
    pattern_checked_mut_ptr_offset_le -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
        #[meta($T:ty, $U:ty)]
        fn $pattern(..) -> _ = mir! {
            let $index: $U = _;
            #[export(ptr)]
            let $ptr: *mut $T = _;
            let $cmp: bool = Le(copy $index, _);
            #[export(offset)]
            let $ptr_1: *mut $T = Offset(copy $ptr, _);
        }
    }
}

template! {
    pattern_checked_mut_ptr_offset_gt -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
        #[meta($T:ty, $U:ty)]
        fn $pattern(..) -> _ = mir! {
            let $index: $U = _;
            #[export(ptr)]
            let $ptr: *mut $T = _;
            let $cmp: bool = Gt(_, copy $index);
            #[export(offset)]
            let $ptr_1: *mut $T = Offset(copy $ptr, _);
        }
    }
}

template! {
    pattern_checked_mut_ptr_offset_ge -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
        #[meta($T:ty, $U:ty)]
        fn $pattern(..) -> _ = mir! {
            let $index: $U = _;
            #[export(ptr)]
            let $ptr: *mut $T = _;
            let $cmp: bool = Ge(_, copy $index);
            #[export(offset)]
            let $ptr_1: *mut $T = Offset(copy $ptr, _);
            let $value: &$T = &(*$ptr_1);
        }
    }
}

template! {
    pattern_checked_mut_ptr_offset_rem -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
        #[meta($T:ty, $U:ty)]
        fn $pattern(..) -> _ = mir! {
            #[export(ptr)]
            let $ptr: *mut $T = _;
            let $index: $U = Rem(_, _);
            #[export(offset)]
            let $ptr_1: *mut $T = Offset(copy $ptr, copy $index);
            let $value: &$T = &(*$ptr_1);
        }
    }
}

struct PatternUncheckedPtrOffsetConst<'pcx> {
    pattern: &'pcx pat::Pattern<'pcx>,
    fn_pat: &'pcx pat::Fn<'pcx>,
    ptr: pat::Location,
    offset: pat::Location,
    const_size: pat::ConstVar<'pcx>,
    const_offset: pat::ConstVar<'pcx>,
}

template! {
    pattern_checked_ptr_offset_const -> PatternUncheckedPtrOffsetConst { ptr, offset, const_size, const_offset } {
        #[meta($T:ty, #[export(const_size)] $size: const(usize), #[export(const_offset)] $offset: const(usize))]
        fn $pattern(..) -> _ = mir! {
            let $array: &[$T; $size] = _; // _1
            let $slice_ref: &[$T] = copy $array as &[$T] (PointerCoercion(Unsize, Implicit)); // _3 bb0[0]
            let $slice_ptr: *const [$T] = &raw const (*$slice_ref); // _5 bb0[1]
            #[export(ptr)]
            let $ptr: *const $T = move $slice_ptr as *const $T (PtrToPtr); // _2 bb0[2]
            #[export(offset)]
            let $ptr_1: *const $T = Offset(copy $ptr, const $offset); // _4 bb0[4]
            let $value: &$T = &(*$ptr_1); // _0 bb0[5]
        }
    }
}

template! {
    pattern_checked_mut_ptr_offset_const -> PatternUncheckedPtrOffsetConst { ptr, offset, const_size, const_offset } {
        #[meta($T:ty, #[export(const_size)] $size: const(usize), #[export(const_offset)] $offset: const(usize))]
        fn $pattern(..) -> _ = mir! {
            let $array: &mut [$T; $size] = _; // _1
            let $slice_ref: &mut [$T] = copy $array as &mut [$T] (PointerCoercion(Unsize, Implicit)); // _3 bb0[0]
            let $slice_ptr: *mut [$T] = &raw mut (*$slice_ref); // _5 bb0[1]
            #[export(ptr)]
            let $ptr: *mut $T = move $slice_ptr as *mut $T (PtrToPtr); // _2 bb0[2]
            #[export(offset)]
            let $ptr_1: *mut $T = Offset(copy $ptr, _); // _4 bb0[4]
        }
    }
}

template! {
    pattern_checked_mut_ptr_offset_slice_len -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
        #[meta($T:ty, $U:ty)]
        fn $pattern(..) -> _ = mir! {
            let $slice_ptr: *mut [$T] = _;
            #[export(ptr)]
            let $ptr: *mut $T = move $slice_ptr as *mut $T (PtrToPtr);
            let $index: $U = PtrMetadata(_);
            #[export(offset)]
            let $ptr_1: *mut $T = Offset(copy $ptr, copy $index);
        }
    }
}
