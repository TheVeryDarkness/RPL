use std::fmt;
use std::sync::LazyLock;

use derive_more::Debug;
use parser::pairs;
use pest_typed::{ParsableTypedNode, Span};
use rustc_middle::mir;

use crate::collect_elems_separated_by_comma;

pub struct Path<'i, Ident = &'i pairs::Identifier<'i>> {
    pub leading: Option<&'i pairs::PathLeading<'i>>,
    pub segments: Vec<(Ident, Vec<&'i pairs::GenericArgument<'i>>)>,
    pub _span: Span<'i>,
}

impl<'i, Ident: From<&'i pairs::Identifier<'i>>> From<&'i pairs::Path<'i>> for Path<'i, Ident> {
    fn from(path: &'i pairs::Path<'i>) -> Self {
        fn collect_args<'i>(args: Option<&'i pairs::PathArguments<'i>>) -> Vec<&'i pairs::GenericArgument<'i>> {
            args.map(|args| {
                let args = args.get_matched().2;
                collect_elems_separated_by_comma!(args).collect()
            })
            .unwrap_or_default()
        }
        let (leading, seg, segs) = path.get_matched();
        let mut segments = vec![(Ident::from(seg.Identifier()), collect_args(seg.PathArguments()))];
        segs.iter_matched().for_each(|seg| {
            let (_, seg) = seg.get_matched();
            let args = collect_args(seg.PathArguments());
            segments.push((Ident::from(seg.Identifier()), args));
        });
        let span = path.span;
        Self {
            leading: leading.as_ref(),
            segments,
            _span: span,
        }
    }
}

impl<'i, Ident: Copy> Path<'i, Ident> {
    /// Returns `Some(ident)` if `self` is a single identifier.
    pub fn as_ident(&self) -> Option<Ident> {
        if self.leading.is_none() && self.segments.len() == 1 && self.segments[0].1.is_empty() {
            // If the path has no leading identifier and only one segment with no generic arguments,
            // it can be treated as a single identifier.
            Some(self.segments[0].0)
        } else {
            None
        }
    }
    /// Returns the last segment.
    pub fn ident(&self) -> Ident {
        //FIXME: use a non-empty `Vec` type
        let last = self.segments.last().unwrap();
        last.0
    }

    /// Returns the leading identifier if it's the path is not starting with `::`.
    pub fn leading_ident(&self) -> Option<Ident> {
        if self.leading.is_none() {
            Some(self.segments[0].0)
        } else {
            None
        }
    }
    /// Replaces the leading identifier with a new one.
    pub fn replace_leading_ident(mut self, mut prefix: Self) -> Self {
        assert!(self.leading.is_none());
        prefix.segments.reserve(self.segments.len().saturating_sub(1));
        assert!(!prefix.segments.is_empty());
        prefix
            .segments
            .last_mut()
            .unwrap()
            .1
            .extend(std::mem::take(&mut self.segments[0].1));
        prefix.segments.extend(self.segments.into_iter().skip(1));
        prefix._span = self._span;
        prefix
    }
}

impl<Ident: fmt::Display> fmt::Debug for Path<'_, Ident> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(leading) = self.leading {
            write!(f, "{}", leading.span.as_str().trim())?;
        }
        for (i, segment) in self.segments.iter().enumerate() {
            if i > 0 {
                write!(f, "::")?;
            }
            write!(f, "{}", segment.0)?;
            if !segment.1.is_empty() {
                write!(f, "::<")?;
                for (j, arg) in segment.1.iter().enumerate() {
                    if j > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg.span.as_str().trim())?;
                }
                write!(f, ">")?;
            }
        }
        Ok(())
    }
}

impl<Ident: fmt::Display> fmt::Display for Path<'_, Ident> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(self, f)
    }
}

#[macro_export]
macro_rules! collect_elems_separated_by_comma {
    ($decls:expr) => {{
        let (first, following, _) = $decls.get_matched();
        let following = following
            .iter_matched()
            .map(|comma_with_elem| comma_with_elem.get_matched().1);
        std::iter::once(first).chain(following)
    }};
}

pub trait Record: Sized {
    type Ok;
    type Err;

    fn or_record(self, errors: &mut Vec<Self::Err>) -> Option<Self::Ok>;
    fn or_record_and_default(self, errors: &mut Vec<Self::Err>) -> Self::Ok
    where
        Self::Ok: Default,
    {
        self.or_record(errors).unwrap_or_default()
    }
}

impl<T, E> Record for Result<T, E> {
    type Ok = T;
    type Err = E;

    fn or_record(self, errors: &mut Vec<<Self as Record>::Err>) -> Option<<Self as Record>::Ok> {
        match self {
            Ok(value) => Some(value),
            Err(err) => {
                errors.push(err);
                None
            },
        }
    }
}

pub fn self_param_ty<'i>(self_param: &'i pairs::SelfParam<'i>) -> (&'static pairs::Type<'static>, mir::Mutability) {
    static SELF: LazyLock<pairs::Type<'static>> = LazyLock::new(|| pairs::Type::try_parse("Self").unwrap());
    static REF_SELF: LazyLock<pairs::Type<'static>> = LazyLock::new(|| pairs::Type::try_parse("&Self").unwrap());
    static REF_MUT_SELF: LazyLock<pairs::Type<'static>> =
        LazyLock::new(|| pairs::Type::try_parse("&mut Self").unwrap());
    if self_param.And().is_some() {
        if self_param.Mutability().kw_mut().is_some() {
            (&REF_MUT_SELF, mir::Mutability::Mut)
        } else {
            (&REF_SELF, mir::Mutability::Not)
        }
    } else {
        (
            &SELF,
            if self_param.Mutability().kw_mut().is_some() {
                mir::Mutability::Mut
            } else {
                mir::Mutability::Not
            },
        )
    }
}
