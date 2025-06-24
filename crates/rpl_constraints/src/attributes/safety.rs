use rpl_parser::pairs;
use rustc_hir::{self as hir};

/// See [`hir::Safety`]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Safety {
    #[default]
    Safe,
    Unsafe,
    Any,
}

impl Safety {
    #[instrument(level = "debug", ret)]
    pub fn check(self, safety: hir::Safety) -> bool {
        match safety {
            hir::Safety::Safe => matches!(self, Self::Safe | Self::Any),
            hir::Safety::Unsafe => matches!(self, Self::Unsafe | Self::Any),
        }
    }
    #[instrument(level = "debug", ret)]
    pub fn check_header(self, safety: hir::HeaderSafety) -> bool {
        match safety {
            hir::HeaderSafety::Normal(safety) => self.check(safety),
            hir::HeaderSafety::SafeTargetFeatures => matches!(self, Self::Unsafe | Self::Any),
        }
    }
    #[instrument(level = "debug", ret)]
    pub fn check_option_header(self, safety: Option<hir::HeaderSafety>) -> bool {
        match safety {
            Some(safety) => self.check_header(safety),
            None => matches!(self, Self::Safe | Self::Any),
        }
    }
    pub fn parse(safety: Option<&pairs::Safety>) -> Self {
        if let Some(safety) = safety {
            let (_, question) = safety.get_matched();
            if question.is_some() {
                Safety::Any
            } else {
                Safety::Unsafe
            }
        } else {
            Safety::Safe
        }
    }
}
