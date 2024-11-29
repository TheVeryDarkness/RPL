use super::*;

pub use rustc_middle::mir::visit::{MutatingUseContext, NonMutatingUseContext, PlaceContext};

pub trait PatternVisitor<'tcx>: Sized {
    fn visit_local(&mut self, _local: LocalIdx, _pcx: PlaceContext, _location: Location) {}
    fn visit_scalar_int(&mut self, _scalar_int: IntValue) {}
    fn visit_ty_var(&mut self, _ty_var: TyVar<'tcx>) {}

    fn visit_const_var(&mut self, const_var: ConstVar<'tcx>) {
        const_var.visit_with(self);
    }
    fn visit_ty(&mut self, ty: Ty<'tcx>) {
        ty.visit_with(self);
    }
    fn visit_const(&mut self, konst: Const<'tcx>) {
        konst.visit_with(self);
    }
    fn visit_generic_args(&mut self, args: GenericArgsRef<'tcx>) {
        args.visit_with(self);
    }
    fn visit_generic_arg(&mut self, arg: GenericArgKind<'tcx>) {
        arg.visit_with(self);
    }
    fn visit_path(&mut self, path: &Path<'tcx>) {
        path.visit_with(self);
    }
    fn visit_const_operand(&mut self, const_operand: &ConstOperand<'tcx>) {
        const_operand.visit_with(self);
    }

    fn visit_basic_block_data(&mut self, bb: BasicBlock, data: &BasicBlockData<'tcx>) {
        self.super_basic_block_data(bb, data);
    }
    fn visit_place(&mut self, place: Place<'tcx>, pcx: PlaceContext, location: Location) {
        self.super_place(place, pcx, location);
    }
    fn visit_projection(&mut self, place: Place<'tcx>, pcx: PlaceContext, location: Location) {
        self.super_projection(place, pcx, location);
    }

    fn visit_projection_elem(
        &mut self,
        place_ref: Place<'tcx>,
        elem: PlaceElem<'tcx>,
        pcx: PlaceContext,
        location: Location,
    ) {
        self.super_projection_elem(place_ref, elem, pcx, location);
    }
    fn visit_rvalue(&mut self, rvalue: &Rvalue<'tcx>, location: Location) {
        self.super_rvalue(rvalue, location);
    }
    fn visit_operand(&mut self, operand: &Operand<'tcx>, location: Location) {
        self.super_operand(operand, location);
    }
    fn visit_statement(&mut self, statement: &StatementKind<'tcx>, location: Location) {
        self.super_statement(statement, location);
    }
    fn visit_terminator(&mut self, terminator: &TerminatorKind<'tcx>, location: Location) {
        self.super_terminator(terminator, location);
    }
    fn visit_switch_targets(&mut self, _targets: &SwitchTargets, _location: Location) {}

    fn super_basic_block_data(&mut self, block: BasicBlock, data: &BasicBlockData<'tcx>) {
        for (statement_index, statement) in data.statements.iter().enumerate() {
            self.visit_statement(statement, Location { block, statement_index });
        }
        if let Some(terminator) = &data.terminator {
            self.visit_terminator(
                terminator,
                Location {
                    block,
                    statement_index: data.statements.len(),
                },
            );
        }
    }
    fn super_place(&mut self, place: Place<'tcx>, pcx: PlaceContext, location: Location) {
        let mut pcx = pcx;

        if !place.projection.is_empty() && pcx.is_use() {
            // ^ Only change the context if it is a real use, not a "use" in debuginfo.
            pcx = if pcx.is_mutating_use() {
                PlaceContext::MutatingUse(MutatingUseContext::Projection)
            } else {
                PlaceContext::NonMutatingUse(NonMutatingUseContext::Projection)
            };
        }

        self.visit_local(place.local, pcx, location);

        self.visit_projection(place, pcx, location);
    }
    fn super_projection(&mut self, place: Place<'tcx>, pcx: PlaceContext, location: Location) {
        for (base, elem) in place.iter_projections().rev() {
            self.visit_projection_elem(base, elem, pcx, location);
        }
    }

    fn super_projection_elem(
        &mut self,
        _place_ref: Place<'tcx>,
        elem: PlaceElem<'tcx>,
        _context: PlaceContext,
        location: Location,
    ) {
        match elem {
            PlaceElem::OpaqueCast(ty) | PlaceElem::Subtype(ty) => {
                self.visit_ty(ty);
            },
            PlaceElem::Index(local) => {
                self.visit_local(
                    local,
                    PlaceContext::NonMutatingUse(NonMutatingUseContext::Copy),
                    location,
                );
            },
            PlaceElem::Deref
            | PlaceElem::Subslice {
                from: _,
                to: _,
                from_end: _,
            }
            | PlaceElem::ConstantIndex {
                offset: _,
                min_length: _,
                from_end: _,
            }
            | PlaceElem::Downcast(_)
            | PlaceElem::Field(_) => {},
        }
    }
    fn super_rvalue(&mut self, rvalue: &Rvalue<'tcx>, location: Location) {
        match rvalue {
            Rvalue::Any => {},
            Rvalue::Use(operand) | Rvalue::UnaryOp(_, operand) => self.visit_operand(operand, location),
            &Rvalue::Repeat(ref operand, konst) => {
                self.visit_operand(operand, location);
                self.visit_const(konst);
            },
            &Rvalue::Ref(_region, bk, place) => {
                let ctx = match bk {
                    mir::BorrowKind::Shared => PlaceContext::NonMutatingUse(NonMutatingUseContext::SharedBorrow),
                    mir::BorrowKind::Fake(_) => PlaceContext::NonMutatingUse(NonMutatingUseContext::FakeBorrow),
                    mir::BorrowKind::Mut { .. } => PlaceContext::MutatingUse(MutatingUseContext::Borrow),
                };
                self.visit_place(place, ctx, location);
            },
            &Rvalue::RawPtr(m, place) => {
                let ctx = match m {
                    mir::Mutability::Mut => PlaceContext::MutatingUse(MutatingUseContext::RawBorrow),
                    mir::Mutability::Not => PlaceContext::NonMutatingUse(NonMutatingUseContext::RawBorrow),
                };
                self.visit_place(place, ctx, location);
            },
            &Rvalue::Len(place) | &Rvalue::Discriminant(place) | &Rvalue::CopyForDeref(place) => {
                self.visit_place(
                    place,
                    PlaceContext::NonMutatingUse(NonMutatingUseContext::Inspect),
                    location,
                );
            },
            &Rvalue::Cast(_, ref operand, ty) | &Rvalue::ShallowInitBox(ref operand, ty) => {
                self.visit_operand(operand, location);
                self.visit_ty(ty);
            },
            Rvalue::BinaryOp(_op, box [lhs, rhs]) => {
                self.visit_operand(lhs, location);
                self.visit_operand(rhs, location);
            },
            &Rvalue::NullaryOp(_op, ty) => self.visit_ty(ty),
            Rvalue::Aggregate(_agg_kind, operands) => operands
                .iter()
                .for_each(|operand| self.visit_operand(operand, location)),
        }
    }
    fn super_operand(&mut self, operand: &Operand<'tcx>, location: Location) {
        match operand {
            Operand::Any => {},
            &Operand::Copy(place) => {
                self.visit_place(
                    place,
                    PlaceContext::NonMutatingUse(NonMutatingUseContext::Copy),
                    location,
                );
            },
            &Operand::Move(place) => self.visit_place(
                place,
                PlaceContext::NonMutatingUse(NonMutatingUseContext::Move),
                location,
            ),
            Operand::Constant(const_operand) => self.visit_const_operand(const_operand),
        }
    }
    fn super_statement(&mut self, statement: &StatementKind<'tcx>, location: Location) {
        let store = PlaceContext::MutatingUse(MutatingUseContext::Store);
        match *statement {
            StatementKind::Assign(place, ref rvalue) => {
                self.visit_place(place, store, location);
                self.visit_rvalue(rvalue, location);
            },
        }
    }
    fn super_terminator(&mut self, terminator: &TerminatorKind<'tcx>, location: Location) {
        match *terminator {
            TerminatorKind::Call {
                ref func,
                ref args,
                destination,
                target: _,
            } => {
                self.visit_operand(func, location);
                for arg in args {
                    self.visit_operand(arg, location);
                }
                if let Some(destination) = destination {
                    self.visit_place(
                        destination,
                        PlaceContext::MutatingUse(MutatingUseContext::Call),
                        location,
                    );
                }
            },
            TerminatorKind::Drop { place, target: _ } => {
                self.visit_place(place, PlaceContext::MutatingUse(MutatingUseContext::Drop), location)
            },
            TerminatorKind::SwitchInt {
                ref operand,
                ref targets,
            } => {
                self.visit_operand(operand, location);
                self.visit_switch_targets(targets, location);
            },
            TerminatorKind::Goto(_) | TerminatorKind::Return | TerminatorKind::PatEnd => {},
        }
    }
}

pub trait PatternVisitable<'tcx>: PatternSuperVisitable<'tcx> {
    fn visit_with<V: PatternVisitor<'tcx>>(&self, vis: &mut V) {
        self.super_visit_with(vis);
    }
}

pub trait PatternSuperVisitable<'tcx> {
    fn super_visit_with<V: PatternVisitor<'tcx>>(&self, vis: &mut V);
}

impl<'tcx, P: PatternSuperVisitable<'tcx>> PatternVisitable<'tcx> for P {}

impl<'tcx> PatternSuperVisitable<'tcx> for Ty<'tcx> {
    fn super_visit_with<V: PatternVisitor<'tcx>>(&self, vis: &mut V) {
        match self.kind() {
            &TyKind::TyVar(ty_var) => vis.visit_ty_var(ty_var),
            &TyKind::Array(ty, konst) => {
                vis.visit_ty(ty);
                vis.visit_const(konst);
            },
            &TyKind::Slice(ty) => vis.visit_ty(ty),
            TyKind::Tuple(tys) => tys.iter().for_each(|&ty| vis.visit_ty(ty)),
            &TyKind::Ref(_region, ty, _) => {
                vis.visit_ty(ty);
            },
            &TyKind::RawPtr(ty, _) => vis.visit_ty(ty),
            &TyKind::Path(PathWithArgs { ref path, args }) => {
                vis.visit_path(path);
                vis.visit_generic_args(args);
            },
            TyKind::Uint(_) | TyKind::Int(_) | TyKind::Float(_) | TyKind::Str | TyKind::Bool => {},
        }
    }
}

impl<'tcx> PatternSuperVisitable<'tcx> for GenericArgsRef<'tcx> {
    fn super_visit_with<V: PatternVisitor<'tcx>>(&self, vis: &mut V) {
        self.iter().for_each(|&arg| vis.visit_generic_arg(arg));
    }
}

impl<'tcx> PatternSuperVisitable<'tcx> for Const<'tcx> {
    fn super_visit_with<V: PatternVisitor<'tcx>>(&self, vis: &mut V) {
        match *self {
            Const::ConstVar(const_var) => vis.visit_const_var(const_var),
            Const::Value(int_value) => vis.visit_scalar_int(int_value),
        }
    }
}

impl<'tcx> PatternSuperVisitable<'tcx> for ConstVar<'tcx> {
    fn super_visit_with<V: PatternVisitor<'tcx>>(&self, vis: &mut V) {
        vis.visit_ty(self.ty);
    }
}

impl<'tcx> PatternSuperVisitable<'tcx> for GenericArgKind<'tcx> {
    fn super_visit_with<V: PatternVisitor<'tcx>>(&self, vis: &mut V) {
        match *self {
            GenericArgKind::Lifetime(_region) => {},
            GenericArgKind::Type(ty) => vis.visit_ty(ty),
            GenericArgKind::Const(konst) => vis.visit_const(konst),
        }
    }
}

impl<'tcx> PatternSuperVisitable<'tcx> for Path<'tcx> {
    fn super_visit_with<V: PatternVisitor<'tcx>>(&self, vis: &mut V) {
        match *self {
            Path::Item(_) | Path::LangItem(_) => {},
            Path::TypeRelative(ty, _) => vis.visit_ty(ty),
        }
    }
}

impl<'tcx> PatternSuperVisitable<'tcx> for ConstOperand<'tcx> {
    fn super_visit_with<V: PatternVisitor<'tcx>>(&self, vis: &mut V) {
        match *self {
            ConstOperand::ConstVar(const_var) => vis.visit_const_var(const_var),
            ConstOperand::ScalarInt(int_value) => vis.visit_scalar_int(int_value),
            ConstOperand::ZeroSized(PathWithArgs { ref path, args }) => {
                vis.visit_path(path);
                vis.visit_generic_args(args);
            },
        }
    }
}
