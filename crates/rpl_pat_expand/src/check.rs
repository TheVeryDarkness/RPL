use crate::symbol_table::{AdtDef, CheckError, FnDef, ImplDef};
use crate::SymbolTable;
use quote::ToTokens;
use rpl_pat_syntax::*;
use rustc_span::Symbol;
use syn::parse::Parse;
use syn::Ident;

pub(crate) fn check_pattern(pattern: &Pattern) -> syn::Result<SymbolTable<'_>> {
    rustc_span::create_session_if_not_set_then(rustc_span::edition::LATEST_STABLE_EDITION, |_| {
        CheckCtxt::default().check_pattern(pattern)
    })
}

#[derive(Default)]
struct CheckCtxt<'pat> {
    symbols: SymbolTable<'pat>,
}

struct CheckFnCtxt<'a, 'pat> {
    #[expect(dead_code)]
    attrs: &'pat [syn::Attribute],
    impl_def: Option<&'a mut ImplDef<'pat>>,
    fn_def: &'a mut FnDef<'pat>,
}

struct CheckAdtCtxt<'a, 'pat> {
    #[expect(dead_code)]
    attrs: &'pat [syn::Attribute],
    #[expect(dead_code)]
    adt_def: &'a mut AdtDef<'pat>,
}

impl<'pat> CheckCtxt<'pat> {
    fn check_pattern(mut self, pattern: &'pat Pattern) -> syn::Result<SymbolTable<'pat>> {
        for item in &pattern.items {
            self.check_item(item)?;
        }
        Ok(self.symbols)
    }
    fn check_item(&mut self, item: &'pat Item) -> syn::Result<()> {
        match &item.kind {
            ItemKind::Fn(fn_pat) => self.check_fn(&item.attrs, fn_pat),
            ItemKind::Struct(struct_pat) => self.check_struct(&item.attrs, struct_pat),
            ItemKind::Enum(enum_pat) => self.check_enum(&item.attrs, enum_pat),
            ItemKind::Impl(_impl_pat) => todo!(), // self.check_impl(impl_pat),
        }
    }
    fn check_fn(&mut self, attrs: &'pat [syn::Attribute], fn_pat: &'pat FnPat) -> syn::Result<()> {
        CheckFnCtxt {
            attrs,
            impl_def: None,
            fn_def: self.symbols.add_fn(&fn_pat.sig.ident, None)?,
        }
        .check_fn(fn_pat)
    }
    fn check_struct(&mut self, attrs: &'pat [syn::Attribute], struct_pat: &'pat Struct) -> syn::Result<()> {
        let adt_def = self.symbols.add_struct(&struct_pat.ident)?;
        let _adt_def = CheckAdtCtxt { attrs, adt_def };
        // adt_def.check_struct(struct_pat);
        todo!()
    }
    fn check_enum(&mut self, attrs: &'pat [syn::Attribute], enum_pat: &'pat Enum) -> syn::Result<()> {
        let adt_def = self.symbols.add_enum(&enum_pat.ident)?;
        let _adt_def = CheckAdtCtxt { attrs, adt_def };
        // adt_def.check_enum(enum_pat);
        todo!()
    }
}

impl<'pat> CheckFnCtxt<'_, 'pat> {
    fn check_fn(mut self, fn_pat: &'pat FnPat) -> syn::Result<()> {
        self.check_fn_sig(&fn_pat.sig)?;
        self.check_fn_body(&fn_pat.body)?;
        Ok(())
    }
    fn check_fn_sig(&mut self, sig: &'pat FnSig) -> syn::Result<()> {
        for param in &sig.params {
            self.check_fn_param(param)?;
        }
        self.check_fn_ret(&sig.ret)?;
        Ok(())
    }
    fn check_fn_param(&mut self, param: &'pat FnParam) -> syn::Result<()> {
        match &param.kind {
            FnParamKind::SelfParam(self_arg) => self.check_self_param(self_arg),
            FnParamKind::Param(param) => self.check_normal_param(param),
        }
    }
    fn check_fn_ret(&mut self, ret: &'pat FnRet) -> syn::Result<()> {
        match ret {
            FnRet::Any(..) | FnRet::Ret(ReturnType::Default) => Ok(()),
            FnRet::Ret(ReturnType::Type(_, ty)) => self.check_type(ty),
        }
    }
    fn check_fn_body(&mut self, body: &'pat FnBody) -> syn::Result<()> {
        match body {
            FnBody::Empty(_) => Ok(()),
            FnBody::Mir(mir) => self.check_mir(&mir.mir),
        }
    }
    fn check_self_param(&mut self, self_param: &'pat SelfParam) -> syn::Result<()> {
        self.fn_def.add_self_param(self_param)?;
        if let Some(PunctAnd { value: ty, .. }) = &self_param.ty {
            self.check_type(ty)?;
        }
        Ok(())
    }
    fn check_normal_param(&mut self, param: &'pat NormalParam) -> syn::Result<()> {
        if let Some(ParamPat { ident, .. }) = &param.ident {
            self.fn_def.add_param(ident, &param.ty)?;
        }
        self.check_type(&param.ty)?;
        Ok(())
    }
}

// impl<'pat> CheckAdtCtxt<'_, 'pat> {
//     fn check_struct(&mut self, struct_pat: &Struct) -> syn::Result<()> {
//         self.check_variant(&struct_pat.variant)?;
//         Ok(())
//     }
//     fn check_enum(&mut self, struct_pat: &Struct) -> syn::Result<()> {
//         self.check_variant(&struct_pat.variant)?;
//         Ok(())
//     }
//     fn check_variant(&mut self, variant: &Variant) -> syn::Result<()> {
//         for field in &variant.fields {
//             self.check_field(field)?;
//         }
//         Ok(())
//     }
//     fn check_field(&mut self, field: &Field) -> syn::Result<()> {
//         self.check_ident(&field.ident)?;
//         self.check_type(&field.ty)?;
//         Ok(())
//     }
// }

// impl<'pat> CheckImplCtxt<'_, 'pat> {
//     fn check_impl(&mut self, impl_pat: &Impl) -> syn::Result<()> {
//         match &impl_pat.kind {
//             ImplKind::Trait(path, _) => self.check_path(path)?,
//             ImplKind::Inherent => {},
//         }
//         for item in &impl_pat.items {
//             self.check_impl_item(impl_pat, item)?;
//         }
//         Ok(())
//     }
//     fn check_impl_item(&mut self, impl_pat: &Impl, item: &ImplItem) -> syn::Result<()> {
//         self.check_attrs(&item.attrs)?;
//         match &item.kind {
//             ImplItemKind::Fn(fn_pat) => self.check_fn(Some(impl_pat), fn_pat),
//         }
//     }
//     fn check_attrs(&mut self, attrs: &[syn::Attribute]) -> syn::Result<()> {
//         for attr in attrs {
//             self.check_attr(attr)?;
//         }
//         Ok(())
//     }
//     fn check_attr(&mut self, _attr: &syn::Attribute) -> syn::Result<()> {
//         todo!()
//     }
// }

impl<'pat> CheckFnCtxt<'_, 'pat> {
    fn check_mir(&mut self, mir: &'pat Mir) -> syn::Result<()> {
        for meta in &mir.metas {
            self.check_meta(meta)?;
        }
        for decl in &mir.declarations {
            self.check_decl(decl)?;
        }
        for stmt in &mir.statements {
            self.check_stmt(stmt)?;
        }
        Ok(())
    }
    fn check_meta(&mut self, meta: &'pat Meta) -> syn::Result<()> {
        meta.meta.content.iter().try_for_each(|item| self.check_meta_item(item))
    }
    fn check_meta_item(&mut self, meta_item: &'pat MetaItem) -> syn::Result<()> {
        match &meta_item.kind {
            MetaKind::Ty(ty_var) => self.fn_def.meta.add_ty_var(&meta_item.ident, ty_var)?,
        }
        Ok(())
    }

    fn check_decl(&mut self, decl: &'pat Declaration) -> syn::Result<()> {
        match decl {
            Declaration::TypeDecl(TypeDecl { ty, ident, .. }) => self.fn_def.meta.add_type(ident, ty),
            Declaration::UsePath(UsePath { path, .. }) => self.fn_def.meta.add_path(path),
            Declaration::LocalDecl(LocalDecl { local, ty, init, .. }) => {
                self.fn_def.add_place_local(local, ty)?;
                if let Some(PunctAnd {
                    value: rvalue_or_call, ..
                }) = init
                {
                    self.check_rvalue_or_call(rvalue_or_call)?;
                }
                Ok(())
            },
        }
    }

    fn check_stmt<End: ToTokens + Parse>(&mut self, stmt: &Statement<End>) -> syn::Result<()> {
        // self.check_attrs(&stmt.attrs)?;
        for attr in &stmt.attrs {
            if attr.path().is_ident("export") {
                self.fn_def.meta.add_export(attr.parse_args::<syntax::Export>()?)?;
            }
        }
        self.check_stmt_kind(&stmt.kind)
    }

    fn check_stmt_kind<End: ToTokens + Parse>(&mut self, stmt: &StatementKind<End>) -> syn::Result<()> {
        match &stmt {
            StatementKind::Assign(
                Assign {
                    place, rvalue_or_call, ..
                },
                _,
            ) => {
                self.check_place(place)?;
                self.check_rvalue_or_call(rvalue_or_call)
            },
            StatementKind::Call(CallIgnoreRet { call, .. }, _) => self.check_call(call),
            StatementKind::Drop(Drop { place, .. }, _) => self.check_place(place),
            StatementKind::Control(control, _) => self.check_control(control),
            StatementKind::Loop(Loop { label, block, .. }) => self.check_loop(label.as_ref(), block),
            StatementKind::SwitchInt(switch_int) => self.check_switch_int(switch_int),
        }
    }

    fn check_rvalue_or_call(&self, rvalue_or_call: &RvalueOrCall) -> syn::Result<()> {
        match rvalue_or_call {
            RvalueOrCall::Rvalue(rvalue) => self.check_rvalue(rvalue),
            RvalueOrCall::Call(call) => self.check_call(call),
        }
    }

    fn check_rvalue(&self, rvalue: &Rvalue) -> syn::Result<()> {
        match rvalue {
            Rvalue::Any(_) => Ok(()),
            Rvalue::Use(RvalueUse { operand, .. })
            | Rvalue::UnaryOp(RvalueUnOp { operand, .. })
            | Rvalue::Repeat(RvalueRepeat { operand, .. }) => self.check_operand(operand),
            Rvalue::Ref(RvalueRef { place, .. })
            | Rvalue::RawPtr(RvalueRawPtr { place, .. })
            | Rvalue::Len(RvalueLen { place, .. })
            | Rvalue::Discriminant(RvalueDiscriminant { place, .. }) => self.check_place(place),
            Rvalue::Cast(RvalueCast { operand, ty, .. }) => {
                self.check_operand(operand)?;
                self.check_type(ty)?;
                Ok(())
            },
            Rvalue::BinaryOp(RvalueBinOp { lhs, rhs, .. }) => {
                self.check_operand(lhs)?;
                self.check_operand(rhs)?;
                Ok(())
            },
            Rvalue::NullaryOp(RvalueNullOp { ty, .. }) => self.check_type(ty),
            Rvalue::Aggregate(agg) => self.check_aggregate(agg),
        }
    }

    fn check_call(&self, call: &Call) -> syn::Result<()> {
        self.check_fn_operand(&call.func)?;
        for operand in call.operands.value.iter() {
            self.check_operand(operand)?;
        }
        Ok(())
    }

    fn check_fn_operand(&self, operand: &FnOperand) -> syn::Result<()> {
        match operand {
            FnOperand::Copy(Parenthesized {
                value: OperandCopy { place, .. },
                ..
            })
            | FnOperand::Move(Parenthesized {
                value: OperandMove { place, .. },
                ..
            }) => self.check_place(place),
            FnOperand::Type(path) => self.check_type_path(path),
            FnOperand::LangItem(lang_item) => self.check_lang_item_with_args(lang_item),
        }
    }

    fn check_operand(&self, operand: &Operand) -> syn::Result<()> {
        match operand {
            Operand::Any(_) | Operand::AnyMultiple(_) => Ok(()),
            Operand::Copy(OperandCopy { place, .. }) | Operand::Move(OperandMove { place, .. }) => {
                self.check_place(place)
            },
            Operand::Constant(konst) => self.check_const_operand(konst),
        }
    }

    fn check_const(&self, konst: &Const) -> syn::Result<()> {
        match konst {
            Const::Lit(_) => Ok(()),
            Const::Path(type_path) => {
                if let Some(qself) = &type_path.qself {
                    self.check_type(&qself.ty)?;
                }
                self.check_path(&type_path.path)?;
                Ok(())
            },
        }
    }

    fn check_const_operand(&self, konst: &ConstOperand) -> syn::Result<()> {
        match konst.kind {
            ConstOperandKind::Lit(_) => Ok(()),
            ConstOperandKind::Type(ref type_path) => {
                if let Some(qself) = &type_path.qself {
                    self.check_type(&qself.ty)?;
                }
                self.check_path(&type_path.path)?;
                Ok(())
            },
            ConstOperandKind::LangItem(ref lang_item) => self.check_lang_item_with_args(lang_item),
        }
    }

    fn check_lang_item_with_args(&self, lang_item: &LangItemWithArgs) -> syn::Result<()> {
        let value = lang_item.item.value();
        rustc_hir::LangItem::from_name(Symbol::intern(&value))
            .ok_or_else(|| syn::Error::new_spanned(lang_item, CheckError::UnknownLangItem(value)))?;
        if let Some(args) = &lang_item.args {
            args.args.iter().try_for_each(|arg| self.check_generic_arg(arg))?;
        }
        Ok(())
    }

    fn check_place(&self, place: &Place) -> syn::Result<()> {
        match place {
            Place::Local(local) => self.check_place_local(local),

            Place::Paren(PlaceParen { place, .. })
            | Place::Deref(PlaceDeref { place, .. })
            | Place::Field(PlaceField { place, .. })
            | Place::Subslice(PlaceSubslice { place, .. })
            | Place::DownCast(PlaceDowncast { place, .. }) => self.check_place(place),
            Place::Index(PlaceIndex { place, index, .. }) => {
                self.check_place(place)?;
                self.check_local(index)?;
                Ok(())
            },
            Place::ConstIndex(PlaceConstIndex {
                place,
                index,
                min_length,
                ..
            }) => {
                self.check_place(place)?;
                if index.index >= min_length.index {
                    return Err(syn::Error::new_spanned(
                        index,
                        CheckError::ConstantIndexOutOfBound(index.index, min_length.index),
                    ));
                }
                Ok(())
            },
        }
    }

    fn check_local(&self, ident: &Ident) -> syn::Result<()> {
        self.fn_def.get_local(ident)?;
        Ok(())
    }

    fn check_place_local(&self, local: &PlaceLocal) -> syn::Result<()> {
        self.fn_def.get_place_local(local)?;
        Ok(())
    }

    fn check_aggregate(&self, agg: &RvalueAggregate) -> syn::Result<()> {
        match agg {
            RvalueAggregate::Array(AggregateArray { operands, .. }) => {
                // self.check_type(ty)?;
                for operand in operands.operands.iter() {
                    self.check_operand(operand)?;
                }
                Ok(())
            },
            RvalueAggregate::Tuple(AggregateTuple { operands }) => {
                for operand in operands.value.iter() {
                    self.check_operand(operand)?;
                }
                Ok(())
            },
            RvalueAggregate::AdtStruct(AggregateAdtStruct { adt, fields }) => {
                self.check_path_or_lang_item(adt)?;
                for field in fields.fields.iter() {
                    self.check_operand(&field.operand)?;
                }
                Ok(())
            },
            RvalueAggregate::AdtTuple(AggregateAdtTuple { adt, fields, .. }) => {
                self.check_path(adt)?;
                for field in fields.value.iter() {
                    self.check_operand(field)?;
                }
                Ok(())
            },
            RvalueAggregate::AdtUnit(AggregateAdtUnit { adt }) => self.check_path_or_lang_item(adt),
            RvalueAggregate::RawPtr(AggregateRawPtr { ty, ptr, metadata, .. }) => {
                self.check_type(&ty.ty)?;
                self.check_operand(ptr)?;
                self.check_operand(metadata)?;
                Ok(())
            },
        }
    }

    fn check_type(&self, ty: &Type) -> syn::Result<()> {
        match ty {
            Type::Never(_) => Ok(()),
            Type::Reference(TypeReference { region, ty, .. }) => {
                self.check_region(*region)?;
                self.check_type(ty)?;
                Ok(())
            },
            Type::Array(TypeArray { ty, .. })
            | Type::Group(TypeGroup { ty, .. })
            | Type::Paren(TypeParen { value: ty, .. })
            | Type::Slice(TypeSlice { ty, .. })
            | Type::Ptr(TypePtr { ty, .. }) => self.check_type(ty),
            Type::Path(path) => self.check_type_path(path),
            Type::Tuple(TypeTuple { tys, .. }) => tys.iter().try_for_each(|ty| self.check_type(ty)),
            Type::TyVar(TypeVar { ident, .. }) => {
                _ = self.fn_def.meta.get_ty_var(ident)?;
                Ok(())
            },
            Type::LangItem(lang_item) => self.check_lang_item_with_args(lang_item),
            Type::SelfType(_) if self.impl_def.is_none() => {
                Ok(())
                // FIXME: error here when `impl` pattern is implemented
                // Err(syn::Error::new_spanned(ty, CheckError::SelfTypeOutsideImpl))
            },
            Type::SelfType(_) => Ok(()),
        }
    }

    fn check_type_path(&self, path: &TypePath) -> syn::Result<()> {
        if let Some(qself) = &path.qself {
            self.check_type(&qself.ty)?;
        }
        self.check_path(&path.path)
    }

    fn check_path_or_lang_item(&self, path: &PathOrLangItem) -> syn::Result<()> {
        match path {
            PathOrLangItem::Path(path) => self.check_path(path),
            PathOrLangItem::LangItem(lang_item) => self.check_lang_item_with_args(lang_item),
        }
    }

    fn check_path(&self, path: &Path) -> syn::Result<()> {
        if let Some(ident) = path.as_ident() {
            if !crate::is_primitive(ident) {
                _ = self.fn_def.meta.get_type(ident)?;
            }
        } else {
            for segment in &path.segments {
                self.check_generic_args(&segment.arguments)?;
            }
        }
        Ok(())
    }

    fn check_generic_args(&self, args: &PathArguments) -> syn::Result<()> {
        match args {
            PathArguments::None => Ok(()),
            PathArguments::AngleBracketed(AngleBracketedGenericArguments { args, .. }) => {
                args.iter().try_for_each(|arg| self.check_generic_arg(arg))
            },
        }
    }

    fn check_generic_arg(&self, arg: &GenericArgument) -> syn::Result<()> {
        match arg {
            &GenericArgument::Region(region) => self.check_region(Some(region)),
            GenericArgument::Type(ty) => self.check_type(ty),
            GenericArgument::Const(GenericConst { konst, .. }) => self.check_const(konst),
        }
    }

    fn check_region(&self, _region: Option<Region>) -> syn::Result<()> {
        Ok(())
    }

    fn check_control(&self, _control: &Control) -> syn::Result<()> {
        Ok(())
    }

    fn check_loop(&mut self, _label: Option<&syn::Label>, block: &Block) -> syn::Result<()> {
        self.check_block(block)
    }

    fn check_block(&mut self, block: &Block) -> syn::Result<()> {
        block.statements.iter().try_for_each(|stmt| self.check_stmt(stmt))
    }

    fn check_switch_int(&mut self, switch_int: &SwitchInt) -> syn::Result<()> {
        self.check_operand(&switch_int.operand)?;
        let mut has_otherwise = false;
        for SwitchTarget {
            ref value, ref body, ..
        } in &switch_int.targets
        {
            if let SwitchValue::Underscore(_) = value {
                if has_otherwise {
                    return Err(syn::Error::new_spanned(value, CheckError::MultipleOtherwiseInSwitchInt));
                }
                has_otherwise = true;
            }
            self.check_switch_value(value)?;
            self.check_switch_body(body)?;
        }
        Ok(())
    }

    fn check_switch_body(&mut self, body: &SwitchBody) -> syn::Result<()> {
        match body {
            SwitchBody::Block(block) => self.check_block(block),
            SwitchBody::Statement(stmt, _) => self.check_stmt_kind(stmt),
        }
    }

    fn check_switch_value(&self, value: &SwitchValue) -> syn::Result<()> {
        match value {
            SwitchValue::Bool(_) | SwitchValue::Underscore(_) => {},
            SwitchValue::Int(int) => {
                if int.suffix().trim_start_matches('_').is_empty() {
                    return Err(syn::Error::new_spanned(int, CheckError::MissingSuffixInSwitchInt));
                }
            },
        }
        Ok(())
    }
}