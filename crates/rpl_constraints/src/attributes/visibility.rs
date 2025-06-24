use rpl_parser::pairs;
use rustc_hir::def_id::DefId;
use rustc_middle::ty;

/// See [`ty::Visibility`]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    #[default]
    Unspecified,
    Public,
    Restricted,
    // FIXME: maybe we should have a `Private` variant?
}

impl Visibility {
    #[instrument(level = "debug", ret)]
    pub fn check(self, visibility: ty::Visibility<DefId>) -> bool {
        match self {
            Self::Unspecified => true,
            Self::Public => visibility.is_public(),
            Self::Restricted => !visibility.is_public(),
        }
    }
    pub fn parse(visibility: Option<&pairs::Visibility>) -> Self {
        if let Some(visibility) = visibility {
            let (_, scope) = visibility.get_matched();
            if scope.is_some() {
                Visibility::Restricted
            } else {
                Visibility::Public
            }
        } else {
            Visibility::Unspecified
        }
    }
}
