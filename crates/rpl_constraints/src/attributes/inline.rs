use rustc_hir::Attribute;

#[derive(Debug, Clone, Copy)]
pub enum Inline {
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
    pub fn check<'tcx>(self, mut attr: impl Iterator<Item = &'tcx Attribute>) -> Option<&'tcx Attribute> {
        match self {
            Inline::Normal => attr.next(),
            Inline::Always => attr.find(|attr| {
                trace!(attr = ?attr, "Checking for inline always");
                attr.meta_item_list().is_some_and(|list| {
                    list.first()
                        .is_some_and(|name| name.ident().is_some_and(|ident| ident.name.as_str() == "always"))
                })
            }),
            Inline::Never => attr.find(|attr| {
                trace!(attr = ?attr, "Checking for inline never");
                attr.meta_item_list().is_some_and(|list| {
                    list.first()
                        .is_some_and(|name| name.ident().is_some_and(|ident| ident.name.as_str() == "never"))
                })
            }),
            Inline::Any => attr.find(|attr| {
                trace!(attr = ?attr, "Checking for inline any");
                attr.meta_item_list().is_none_or(|list| {
                    list.first()
                        .is_none_or(|name| name.ident().is_none_or(|ident| ident.name.as_str() != "never"))
                })
            }),
        }
    }
}
