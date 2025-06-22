use rustc_hir::Attribute;

#[derive(Default, Debug, Clone, Copy)]
pub enum Inline {
    #[default]
    Unspecified,
    /// `#[inline]`
    Normal,
    /// `#[inline(always)]`
    Always,
    /// `#[inline(never)]`
    Never,
    /// Not [`Inline::Never`]
    Any,
}

impl Inline {
    #[instrument(level = "debug", skip(attr), ret)]
    pub fn check<'tcx>(self, mut attr: impl Iterator<Item = &'tcx Attribute>) -> bool {
        match self {
            Inline::Unspecified => return true,
            Inline::Normal => return attr.count() > 0,
            Inline::Always => {
                return attr.any(|attr| {
                    trace!(attr = ?attr, "Checking for inline always");
                    attr.meta_item_list().is_some_and(|list| {
                        list.first()
                            .is_some_and(|name| name.ident().is_some_and(|ident| ident.name.as_str() == "always"))
                    })
                });
            },
            Inline::Never => {
                return attr.any(|attr| {
                    trace!(attr = ?attr, "Checking for inline never");
                    attr.meta_item_list().is_some_and(|list| {
                        list.first()
                            .is_some_and(|name| name.ident().is_some_and(|ident| ident.name.as_str() == "never"))
                    })
                });
            },
            Inline::Any => {
                return attr.any(|attr| {
                    trace!(attr = ?attr, "Checking for inline any");
                    attr.meta_item_list().is_none_or(|list| {
                        list.first()
                            .is_none_or(|name| name.ident().is_none_or(|ident| ident.name.as_str() != "never"))
                    })
                });
            },
        }
    }
}
