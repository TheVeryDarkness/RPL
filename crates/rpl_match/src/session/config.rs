//! Configuration for match session solving.

/// Maximum number of session results produced per pattern item (0 = unlimited).
pub const DEFAULT_MAX_SESSION_RESULTS: usize = 256;

#[derive(Debug, Clone, Copy)]
pub struct SessionConfig {
    pub max_results: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            max_results: DEFAULT_MAX_SESSION_RESULTS,
        }
    }
}
