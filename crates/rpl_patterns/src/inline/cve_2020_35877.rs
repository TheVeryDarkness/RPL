use crate::lints::{DEREF_UNCHECKED_PTR_OFFSET, UNCHECKED_POINTER_OFFSET};
use itertools::Itertools;
use rpl_context::PatCtxt;
use rpl_match::TryCmpAs;
use rpl_mir::pat::Location;
use rpl_mir::{local_is_arg, pat, CheckMirCtxt, StatementMatch };
use rustc_hir::def_id::LocalDefId;
use rustc_hir::intravisit::{self, Visitor};
use rustc_hir::{self as hir};
use rustc_middle::hir::nested_filter::All;
use rustc_middle::mir::Body;
use rustc_middle::ty::{TyCtxt, TypingEnv};
use rustc_span::{Span, Symbol};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
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
        let typing_env = TypingEnv::post_analysis(self.tcx, body.source.def_id());

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

            /// `positive` means offset to any pointer, while `positive_deref` means offset and later dereference to a pointer
            fn collect_checked<'m, 'tcx, 'pcx>(
                positive: PatternUncheckedPtrOffsetGeneral<'pcx>,
                positive_deref: PatternUncheckedPtrOffsetDeref<'pcx>,
                negative_len: PatternUncheckedPtrOffsetLen<'pcx>,
                negative_general: &[PatternUncheckedPtrOffsetGeneral<'pcx>],
                negative_const: &[PatternUncheckedPtrOffsetConst<'pcx>],
                def_id: LocalDefId,
                body: &Body<'tcx>,
                tcx: TyCtxt<'tcx>,
                pcx: PatCtxt<'pcx>,
                typing_env: TypingEnv<'tcx>,
            ) {
                // Offset inside the allocated object
                let mut negative_inside: BTreeSet<_> = negative_general
                    .into_iter()
                    .flat_map(move |pattern| {
                        CheckMirCtxt::new(tcx, pcx, body, pattern.pattern, pattern.fn_pat).check()
                            .iter()
                            .map(move |matches| {
                                let ptr = matches[pattern.ptr];
                                let offset = matches[pattern.offset];
                                let span_ptr = ptr.span_no_inline(body);
                                let span_offset = offset.span_no_inline(body);
                                trace!(?ptr, ?offset, ?pattern.ptr, ?pattern.offset, ?span_ptr, ?span_offset, "checked offset found");
                                (ptr, offset)
                            }).collect_vec()
                    }).collect();
                        for matches in CheckMirCtxt::new(tcx, pcx, body, negative_len.pattern, negative_len.fn_pat).check(){
                                let ptr = matches[negative_len.ptr];
                                let offset = matches[negative_len.offset];
                                let span_ptr = ptr.span_no_inline(body);
                                let span_offset = offset.span_no_inline(body);
                                trace!(?ptr, ?offset, pattern.ptr = ?negative_len.ptr, pattern.offset = ?negative_len.offset, ?span_ptr, ?span_offset, "offset len found");
                                if matches[negative_len.index.idx].as_local().is_none_or(|local| !local_is_arg(local, body)) {
                                    negative_inside.insert((ptr,offset));
                                }
                        }
                    // Offset pass the end of the allocated object
                    let mut negative_at_end: BTreeSet<_> = BTreeSet::new();
                 for pattern in negative_const {
                        let matches=CheckMirCtxt::new(tcx, pcx, body, pattern.pattern, pattern.fn_pat).check();
                        for matches in matches{
                            let const_size = matches[pattern.const_size.idx];
                            let const_offset = matches[pattern.const_offset.idx];
                                let ptr = matches[pattern.ptr];
                                let offset = matches[pattern.offset];
                                let span_ptr = ptr.span_no_inline(body);
                                let span_offset = offset.span_no_inline(body);
                                trace!(?ptr, ?offset, ?pattern.ptr, ?pattern.offset, ?span_ptr, ?span_offset, "offset const found");
                            let cmp = const_offset.try_cmp_as(const_size, tcx, typing_env);
                            match cmp {
                                Some(Ordering::Less) => {negative_inside.insert((ptr,offset));},
                                Some(Ordering::Equal) => {negative_at_end.insert((ptr,offset));},
                                // The offset is out of bounds, so this is not a valid negative case
                                _ => (),
                            }
                        }
                    }
                let mut positives: BTreeMap<(StatementMatch, StatementMatch), Option<StatementMatch>>= BTreeMap::new();

                 for matches in CheckMirCtxt::new(tcx, pcx, body, positive.pattern, positive.fn_pat).check(){
                    let ptr = matches[positive.ptr];
                    let offset = matches[positive.offset];
                    if !negative_inside.contains(&(ptr, offset)) {
                        positives.insert((ptr,offset), None);
                    } else {
                        // The offset is checked, so don't emit an error
                    }
                }

                 for matches in CheckMirCtxt::new(tcx, pcx, body, positive_deref.pattern, positive_deref.fn_pat).check(){
                    let ptr = matches[positive_deref.ptr];
                    let offset = matches[positive_deref.offset];
                    let deref = matches[positive_deref.deref];
                    if !negative_inside.contains(&(ptr, offset)) && !negative_at_end.contains(&(ptr, offset)) {
                        positives.insert((ptr,offset), Some(deref));
                    } else {
                        // The offset is checked, so don't emit an error
                    }
                }

                for ((ptr,offset), _) in positives {
                    let span_ptr = ptr.span_no_inline(body);
                    let span_offset = offset.span_no_inline(body);
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
            macro_rules! collect_checked {
                ($module:ident) => {
                    collect_checked(
                        $module::pattern_unchecked_ptr_offset_(self.pcx),
                        $module::pattern_unchecked_ptr_offset_deref(self.pcx),
                        $module::pattern_checked_ptr_offset_vec_len(self.pcx),
                        &[
                            $module::pattern_checked_ptr_offset_lt(self.pcx),
                            $module::pattern_checked_ptr_offset_le(self.pcx),
                            $module::pattern_checked_ptr_offset_gt(self.pcx),
                            $module::pattern_checked_ptr_offset_ge(self.pcx),
                            $module::pattern_checked_ptr_offset_rem(self.pcx),
                            $module::pattern_checked_ptr_offset_slice_len(self.pcx),
                        ],
                        &[
                            $module::pattern_checked_ptr_offset_const(self.pcx),
                            $module::pattern_checked_ptr_offset_copy_const(self.pcx),
                        ],
                        def_id,
                        body,
                        self.tcx,
                        self.pcx,
                        typing_env,
                    );
                };
            }
            
            collect_checked!(mut_offset);
            collect_checked!(const_offset);
            collect_checked!(mut_arith_offset);
            collect_checked!(const_arith_offset);
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
        #[instrument(level = "trace", skip(pcx))]
        pub(crate) fn $name(pcx: PatCtxt<'_>) -> $ret<'_> {
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

// template! {
//     pattern_unchecked_ptr_offset_ -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
//         #[meta($T:ty)]
//         fn $pattern(..) -> _ = mir! {
//             #[export(ptr)]
//             let $ptr: *const $T = _;
//             let $ptr_1: *const $T;
//             #[export(offset)]
//             $ptr_1 = Offset(copy $ptr, _);
//         }
//     }
// }

// template! {
//     pattern_unchecked_mut_ptr_offset_ -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
//         #[meta($T:ty)]
//         fn $pattern(..) -> _ = mir! {
//             #[export(ptr)]
//             let $ptr: *mut $T = _;
//             let $ptr_1: *mut $T;
//             #[export(offset)]
//             $ptr_1 = Offset(copy $ptr, _);
//         }
//     }
// }

struct PatternUncheckedPtrOffsetDeref<'pcx> {
    pattern: &'pcx pat::Pattern<'pcx>,
    fn_pat: &'pcx pat::Fn<'pcx>,
    ptr: pat::Location,
    offset: pat::Location,
    deref: pat::Location,
}

// template! {
//     pattern_unchecked_ptr_offset_deref -> PatternUncheckedPtrOffsetDeref { ptr, offset, deref } {
//         #[meta($T:ty)]
//         fn $pattern(..) -> _ = mir! {
//             #[export(ptr)]
//             let $ptr: *const $T = _;
//             #[export(offset)]
//             let $ptr_1: *const $T = Offset(copy $ptr, _);
//             #[export(deref)]
//             let $value: &$T = &(*$ptr_1); // _0 bb0[5]
//         }
//     }
// }

// template! {
//     pattern_unchecked_mut_ptr_offset_deref -> PatternUncheckedPtrOffsetDeref { ptr, offset, deref } {
//         #[meta($T:ty)]
//         fn $pattern(..) -> _ = mir! {
//             #[export(ptr)]
//             let $ptr: *mut $T = _;
//             #[export(offset)]
//             let $ptr_1: *mut $T = Offset(copy $ptr, _);
//             #[export(deref)]
//             let $value: &mut $T = &mut (*$ptr_1); // _0 bb0[5]
//         }
//     }
// }

// template! {
//     pattern_checked_ptr_offset_lt -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
//         #[meta($T:ty, $U:ty)]
//         fn $pattern(..) -> _ = mir! {
//             let $index: $U;
//             #[export(ptr)]
//             let $ptr: *const $T = _;
//             let $cmp: bool = Lt(copy $index, _);
//             let $ptr_1: *const $T;
//             #[export(offset)]
//             $ptr_1 = Offset(copy $ptr, _);
//         }
//     }
// }

// template! {
//     pattern_checked_mut_ptr_offset_lt -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
//         #[meta($T:ty, $U:ty)]
//         fn $pattern(..) -> _ = mir! {
//             let $index: $U = _;
//             #[export(ptr)]
//             let $ptr: *mut $T = _;
//             let $cmp: bool = Lt(copy $index, _);
//             let $ptr_1: *mut $T;
//             #[export(offset)]
//             $ptr_1 = Offset(copy $ptr, _);
//         }
//     }
// }

// template! {
//     pattern_checked_ptr_offset_le -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
//         #[meta($T:ty, $U:ty)]
//         fn $pattern(..) -> _ = mir! {
//             let $index: $U;
//             #[export(ptr)]
//             let $ptr: *const $T = _;
//             let $cmp: bool = Le(copy $index, _);
//             let $ptr_1: *const $T;
//             #[export(offset)]
//             $ptr_1 = Offset(copy $ptr, _);
//         }
//     }
// }

// template! {
//     pattern_checked_mut_ptr_offset_le -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
//         #[meta($T:ty, $U:ty)]
//         fn $pattern(..) -> _ = mir! {
//             let $index: $U = _;
//             #[export(ptr)]
//             let $ptr: *mut $T = _;
//             let $cmp: bool = Le(copy $index, _);
//             let $ptr_1: *mut $T;
//             #[export(offset)]
//             $ptr_1 = Offset(copy $ptr, _);
//         }
//     }
// }

// template! {
//     pattern_checked_ptr_offset_gt -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
//         #[meta($T:ty, $U:ty)]
//         fn $pattern(..) -> _ = mir! {
//             let $index: $U;
//             #[export(ptr)]
//             let $ptr: *const $T = _;
//             let $cmp: bool = Gt(_, copy $index);
//             let $ptr_1: *const $T;
//             #[export(offset)]
//             $ptr_1 = Offset(copy $ptr, _);
//         }
//     }
// }

// template! {
//     pattern_checked_mut_ptr_offset_gt -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
//         #[meta($T:ty, $U:ty)]
//         fn $pattern(..) -> _ = mir! {
//             let $index: $U = _;
//             #[export(ptr)]
//             let $ptr: *mut $T = _;
//             let $cmp: bool = Gt(_, copy $index);
//             let $ptr_1: *mut $T;
//             #[export(offset)]
//             $ptr_1 = Offset(copy $ptr, _);
//         }
//     }
// }

// template! {
//     pattern_checked_ptr_offset_ge -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
//         #[meta($T:ty, $U:ty)]
//         fn $pattern(..) -> _ = mir! {
//             let $index: $U;
//             #[export(ptr)]
//             let $ptr: *const $T = _;
//             let $cmp: bool = Ge(_, copy $index);
//             let $ptr_1: *const $T;
//             #[export(offset)]
//             $ptr_1 = Offset(copy $ptr, _);
//         }
//     }
// }

// template! {
//     pattern_checked_mut_ptr_offset_ge -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
//         #[meta($T:ty, $U:ty)]
//         fn $pattern(..) -> _ = mir! {
//             let $index: $U = _;
//             #[export(ptr)]
//             let $ptr: *mut $T = _;
//             let $cmp: bool = Ge(_, copy $index);
//             let $ptr_1: *mut $T;
//             #[export(offset)]
//             $ptr_1 = Offset(copy $ptr, _);
//         }
//     }
// }

// template! {
//     pattern_checked_ptr_offset_rem -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
//         #[meta($T:ty, $U:ty)]
//         fn $pattern(..) -> _ = mir! {
//             #[export(ptr)]
//             let $ptr: *const $T = _;
//             let $index: $U;
//             let $ptr_1: *const $T;
//             $index = Rem(_, _);
//             #[export(offset)]
//             $ptr_1 = Offset(copy $ptr, copy $index);
//         }
//     }
// }

// template! {
//     pattern_checked_mut_ptr_offset_rem -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
//         #[meta($T:ty, $U:ty)]
//         fn $pattern(..) -> _ = mir! {
//             #[export(ptr)]
//             let $ptr: *mut $T = _;
//             let $index: $U = Rem(_, _);
//             let $ptr_1: *mut $T;
//             #[export(offset)]
//             $ptr_1 = Offset(copy $ptr, copy $index);
//         }
//     }
// }

// template! {
//     pattern_checked_ptr_offset_slice_len -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
//         #[meta($T:ty, $U:ty)]
//         fn $pattern(..) -> _ = mir! {
//             let $slice_ptr: *const [$T] = _; // _5
//             #[export(ptr)]
//             let $ptr: *const $T = move $slice_ptr as *const $T (PtrToPtr); // _3
//             let $index: $U = PtrMetadata(_); // _4
//             #[export(offset)]
//             let $ptr_1: *const $T = Offset(copy $ptr, copy $index); //copy $offset_0
//         }
//     }
// }

// template! {
//     pattern_checked_mut_ptr_offset_slice_len -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
//         #[meta($T:ty, $U:ty)]
//         fn $pattern(..) -> _ = mir! {
//             let $slice_ptr: *mut [$T] = _;
//             #[export(ptr)]
//             let $ptr: *mut $T = move $slice_ptr as *mut $T (PtrToPtr);
//             let $index: $U = PtrMetadata(_);
//             #[export(offset)]
//             let $ptr_1: *mut $T = Offset(copy $ptr, copy $index);
//         }
//     }
// }

struct PatternUncheckedPtrOffsetLen<'pcx> {
    pattern: &'pcx pat::Pattern<'pcx>,
    fn_pat: &'pcx pat::Fn<'pcx>,
    ptr: pat::Location,
    offset: pat::Location,
    index: pat::PlaceVar<'pcx>,
}

// template! {
//     pattern_checked_ptr_offset_vec_len -> PatternUncheckedPtrOffsetLen { ptr, offset, index } {
//         #[meta($T:ty, $U:ty, #[export(index)] $index: place($U))]
//         fn $pattern(..) -> _ = mir! {
//             #[export(ptr)]
//             let $ptr: *const $T = _; // _3
//             #[export(offset)]
//             let $ptr_1: *const $T = Offset(copy $ptr, copy $index); //copy $offset_0
//         }
//     }
// }

// template! {
//     pattern_checked_mut_ptr_offset_vec_len -> PatternUncheckedPtrOffsetLen { ptr, offset, index } {
//         #[meta($T:ty, $U:ty, #[export(index)] $index: place($U))]
//         fn $pattern(..) -> _ = mir! {
//             #[export(ptr)]
//             let $ptr: *mut $T = _;
//             #[export(offset)]
//             let $ptr_1: *mut $T = Offset(copy $ptr, copy $index);
//         }
//     }
// }

struct PatternUncheckedPtrOffsetConst<'pcx> {
    pattern: &'pcx pat::Pattern<'pcx>,
    fn_pat: &'pcx pat::Fn<'pcx>,
    ptr: pat::Location,
    offset: pat::Location,
    const_size: pat::ConstVar<'pcx>,
    const_offset: pat::ConstVar<'pcx>,
}

// template! {
//     pattern_checked_ptr_offset_const -> PatternUncheckedPtrOffsetConst { ptr, offset, const_size, const_offset } {
//         #[meta($T:ty, #[export(const_size)] $size: const(usize), #[export(const_offset)] $offset: const(usize))]
//         fn $pattern(..) -> _ = mir! {
//             let $array: &[$T; $size]; // _1
//             let $slice_ref: &[$T] = copy $array as &[$T] (PointerCoercion(Unsize, Implicit)); // _3 bb0[0]
//             let $slice_ptr: *const [$T] = &raw const (*$slice_ref); // _5 bb0[1]
//             #[export(ptr)]
//             let $ptr: *const $T = move $slice_ptr as *const $T (PtrToPtr); // _2 bb0[2]
//             #[export(offset)]
//             let $ptr_1: *const $T = Offset(copy $ptr, const $offset); // _4 bb0[4]
//             // let $value: &$T = &(*$ptr_1); // _0 bb0[5]
//         }
//     }
// }

// template! {
//     pattern_checked_mut_ptr_offset_const -> PatternUncheckedPtrOffsetConst { ptr, offset, const_size, const_offset } {
//         #[meta($T:ty, #[export(const_size)] $size: const(usize), #[export(const_offset)] $offset: const(usize))]
//         fn $pattern(..) -> _ = mir! {
//             let $array: &mut [$T; $size]; // _1
//             let $slice_ref: &mut [$T] = copy $array as &mut [$T] (PointerCoercion(Unsize, Implicit)); // _3 bb0[0]
//             let $slice_ptr: *mut [$T] = &raw mut (*$slice_ref); // _5 bb0[1]
//             #[export(ptr)]
//             let $ptr: *mut $T = move $slice_ptr as *mut $T (PtrToPtr); // _2 bb0[2]
//             #[export(offset)]
//             let $ptr_1: *mut $T = Offset(copy $ptr, const $offset); // _4 bb0[4]
//             // let $value: &mut $T = &mut (*$ptr_1); // _0 bb0[5]
//         }
//     }
// }

// template! {
//     pattern_checked_ptr_offset_copy_const -> PatternUncheckedPtrOffsetConst { ptr, offset, const_size, const_offset } {
//         #[meta($T:ty, #[export(const_size)] $size: const(usize), #[export(const_offset)] $offset: const(usize))]
//         fn $pattern(..) -> _ = mir! {
//             let $array: &[$T; $size]; // _1
//             let $slice_ref: &[$T] = copy $array as &[$T] (PointerCoercion(Unsize, Implicit)); // _3 bb0[0]
//             let $slice_ptr: *const [$T] = &raw const (*$slice_ref); // _5 bb0[1]
//             #[export(ptr)]
//             let $ptr: *const $T = move $slice_ptr as *const $T (PtrToPtr); // _2 bb0[2]
//             let $offset_local: usize = const $offset;
//             #[export(offset)]
//             let $ptr_1: *const $T = Offset(copy $ptr, copy $offset_local); // _4 bb0[4]
//             // let $value: &$T = &(*$ptr_1); // _0 bb0[5]
//         }
//     }
// }

// template! {
//     pattern_checked_mut_ptr_offset_copy_const -> PatternUncheckedPtrOffsetConst { ptr, offset, const_size, const_offset } {
//         #[meta($T:ty, #[export(const_size)] $size: const(usize), #[export(const_offset)] $offset: const(usize))]
//         fn $pattern(..) -> _ = mir! {
//             let $array: &mut [$T; $size]; // _1
//             let $slice_ref: &mut [$T] = copy $array as &mut [$T] (PointerCoercion(Unsize, Implicit)); // _3 bb0[0]
//             let $slice_ptr: *mut [$T] = &raw mut (*$slice_ref); // _5 bb0[1]
//             #[export(ptr)]
//             let $ptr: *mut $T = move $slice_ptr as *mut $T (PtrToPtr); // _2 bb0[2]
//             let $offset_local: usize = const $offset;
//             #[export(offset)]
//             let $ptr_1: *mut $T = Offset(copy $ptr, copy $offset_local); // _4 bb0[4]
//             // let $value: &mut $T = &mut (*$ptr_1); // _0 bb0[5]
//         }
//     }
// }

macro_rules! pattern_templates {
    ({$($ref_mtbl:tt)?} {$ptr_mtbl:tt} {$($offset_operand:tt)*}) => {
        template! {
            pattern_unchecked_ptr_offset_ -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
                #[meta($T:ty)]
                fn $pattern(..) -> _ = mir! {
                    #[export(ptr)]
                    let $ptr: *$ptr_mtbl $T = _;
                    let $ptr_1: *$ptr_mtbl $T;
                    #[export(offset)]
                    $ptr_1 = $($offset_operand)*(copy $ptr, _);
                }
            }
        }

        template! {
            pattern_unchecked_ptr_offset_deref -> PatternUncheckedPtrOffsetDeref { ptr, offset, deref } {
                #[meta($T:ty)]
                fn $pattern(..) -> _ = mir! {
                    #[export(ptr)]
                    let $ptr: *$ptr_mtbl $T = _;
                    #[export(offset)]
                    let $ptr_1: *$ptr_mtbl $T = $($offset_operand)*(copy $ptr, _);
                    #[export(deref)]
                    let $value: &$($ref_mtbl)? $T = &$($ref_mtbl)? (*$ptr_1); // _0 bb0[5]
                }
            }
        }

        template! {
            pattern_checked_ptr_offset_lt -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
                #[meta($T:ty, $U:ty)]
                fn $pattern(..) -> _ = mir! {
                    let $index: $U;
                    #[export(ptr)]
                    let $ptr: *$ptr_mtbl $T = _;
                    let $cmp: bool = Lt(copy $index, _);
                    let $ptr_1: *$ptr_mtbl $T;
                    #[export(offset)]
                    $ptr_1 = $($offset_operand)*(copy $ptr, _);
                }
            }
        }

        template! {
            pattern_checked_ptr_offset_le -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
                #[meta($T:ty, $U:ty)]
                fn $pattern(..) -> _ = mir! {
                    let $index: $U;
                    #[export(ptr)]
                    let $ptr: *$ptr_mtbl $T = _;
                    let $cmp: bool = Le(copy $index, _);
                    let $ptr_1: *$ptr_mtbl $T;
                    #[export(offset)]
                    $ptr_1 = $($offset_operand)*(copy $ptr, _);
                }
            }
        }

        template! {
            pattern_checked_ptr_offset_gt -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
                #[meta($T:ty, $U:ty)]
                fn $pattern(..) -> _ = mir! {
                    let $index: $U;
                    #[export(ptr)]
                    let $ptr: *$ptr_mtbl $T = _;
                    let $cmp: bool = Gt(_, copy $index);
                    let $ptr_1: *$ptr_mtbl $T;
                    #[export(offset)]
                    $ptr_1 = $($offset_operand)*(copy $ptr, _);
                }
            }
        }

        template! {
            pattern_checked_ptr_offset_ge -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
                #[meta($T:ty, $U:ty)]
                fn $pattern(..) -> _ = mir! {
                    let $index: $U;
                    #[export(ptr)]
                    let $ptr: *$ptr_mtbl $T = _;
                    let $cmp: bool = Ge(_, copy $index);
                    let $ptr_1: *$ptr_mtbl $T;
                    #[export(offset)]
                    $ptr_1 = $($offset_operand)*(copy $ptr, _);
                }
            }
        }

        template! {
            pattern_checked_ptr_offset_rem -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
                #[meta($T:ty, $U:ty)]
                fn $pattern(..) -> _ = mir! {
                    #[export(ptr)]
                    let $ptr: *$ptr_mtbl $T = _;
                    let $index: $U;
                    let $ptr_1: *$ptr_mtbl $T;
                    $index = Rem(_, _);
                    #[export(offset)]
                    $ptr_1 = $($offset_operand)*(copy $ptr, copy $index);
                }
            }
        }

        template! {
            pattern_checked_ptr_offset_slice_len -> PatternUncheckedPtrOffsetGeneral { ptr, offset } {
                #[meta($T:ty, $U:ty)]
                fn $pattern(..) -> _ = mir! {
                    let $slice_ptr: *$ptr_mtbl [$T] = _; // _5
                    #[export(ptr)]
                    let $ptr: *$ptr_mtbl $T = move $slice_ptr as *$ptr_mtbl $T (PtrToPtr); // _3
                    let $index: $U = PtrMetadata(_); // _4
                    #[export(offset)]
                    let $ptr_1: *$ptr_mtbl $T = $($offset_operand)*(copy $ptr, copy $index); //copy $offset_0
                }
            }
        }

        template! {
            pattern_checked_ptr_offset_vec_len -> PatternUncheckedPtrOffsetLen { ptr, offset, index } {
                #[meta($T:ty, $U:ty, #[export(index)] $index: place($U))]
                fn $pattern(..) -> _ = mir! {
                    #[export(ptr)]
                    let $ptr: *$ptr_mtbl $T = _; // _3
                    #[export(offset)]
                    let $ptr_1: *$ptr_mtbl $T = $($offset_operand)*(copy $ptr, copy $index); //copy $offset_0
                }
            }
        }

        template! {
            pattern_checked_ptr_offset_const -> PatternUncheckedPtrOffsetConst { ptr, offset, const_size, const_offset } {
                #[meta($T:ty, #[export(const_size)] $size: const(usize), #[export(const_offset)] $offset: const(usize))]
                fn $pattern(..) -> _ = mir! {
                    let $array: &$($ref_mtbl)? [$T; $size]; // _1
                    let $slice_ref: &$($ref_mtbl)? [$T] = copy $array as &$($ref_mtbl)? [$T] (PointerCoercion(Unsize, Implicit)); // _3 bb0[0]
                    let $slice_ptr: *$ptr_mtbl [$T] = &raw $ptr_mtbl (*$slice_ref); // _5 bb0[1]
                    #[export(ptr)]
                    let $ptr: *$ptr_mtbl $T = move $slice_ptr as *$ptr_mtbl $T (PtrToPtr); // _2 bb0[2]
                    #[export(offset)]
                    let $ptr_1: *$ptr_mtbl $T = $($offset_operand)*(copy $ptr, const $offset); // _4 bb0[4]
                    // let $value: &$($ref_mtbl)? $T = &$($ref_mtbl)? (*$ptr_1); // _0 bb0[5]
                }
            }
        }

        template! {
            pattern_checked_ptr_offset_copy_const -> PatternUncheckedPtrOffsetConst { ptr, offset, const_size, const_offset } {
                #[meta($T:ty, #[export(const_size)] $size: const(usize), #[export(const_offset)] $offset: const(usize))]
                fn $pattern(..) -> _ = mir! {
                    let $array: &$($ref_mtbl)? [$T; $size]; // _1
                    let $slice_ref: &$($ref_mtbl)? [$T] = copy $array as &$($ref_mtbl)? [$T] (PointerCoercion(Unsize, Implicit)); // _3 bb0[0]
                    let $slice_ptr: *$ptr_mtbl [$T] = &raw $ptr_mtbl (*$slice_ref); // _5 bb0[1]
                    #[export(ptr)]
                    let $ptr: *$ptr_mtbl $T = move $slice_ptr as *$ptr_mtbl $T (PtrToPtr); // _2 bb0[2]
                    let $offset_local: usize = const $offset;
                    #[export(offset)]
                    let $ptr_1: *$ptr_mtbl $T = $($offset_operand)*(copy $ptr, copy $offset_local); // _4 bb0[4]
                    // let $value: &$($ref_mtbl)? $T = &$($ref_mtbl)? (*$ptr_1); // _0 bb0[5]
                }
            }
        }
    };
}

mod mut_offset{
    use super::*;
    pattern_templates!({mut} {mut} {Offset});
}

mod const_offset{
    use super::*;
    pattern_templates!({} {const} {Offset});
}

mod mut_arith_offset{
    use super::*;
    pattern_templates!({mut} {mut} {std::intrinsics::arith_offset});
}

mod const_arith_offset{
    use super::*;
    pattern_templates!({} {const} {std::intrinsics::arith_offset});
}

