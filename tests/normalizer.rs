#![cfg(test)]

use std::fs::read_to_string;
use std::path::Path;

use ui_test::per_test_config::TestConfig;
use ui_test::{Config, Match, error_on_output_conflict};

#[test]
fn normalize_stderr() {
    let mut cfg = Config::rustc(env!("CARGO_MANIFEST_DIR"));
    cfg.comment_defaults
        .base()
        .normalize_stderr
        .push((Match::PathBackslash, b"/".to_vec()));
    let stderr_path = Path::new("tests/normalizer.stderr");
    let test_path = Path::new("tests/normalizer.rs");
    let mut text = read_to_string(stderr_path).unwrap();
    assert!(text.contains('/'));
    text = text.replace("/", "\\");
    let mut errors = Vec::new();
    let test_cfg = TestConfig::one_off_runner(cfg, test_path.to_path_buf());
    error_on_output_conflict(stderr_path, text.as_bytes(), &mut errors, &test_cfg);
    assert!(
        errors.is_empty(),
        "Expected no errors, got: {:?}",
        errors.first().unwrap()
    );
}
