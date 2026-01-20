use serde::Deserialize;

use crate::RplConfig;

#[derive(Debug, Deserialize)]
pub(crate) struct RunConfig {
    #[serde(alias = "inline-mir")]
    pub inline_mir: Option<bool>,
}

pub(crate) fn load_inline_mir(config: Option<&RplConfig>) -> Option<bool> {
    config.and_then(|config| config.run.as_ref().and_then(|run| run.inline_mir))
}
