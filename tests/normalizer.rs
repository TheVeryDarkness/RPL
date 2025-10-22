#![cfg(test)]

use std::fs::read_to_string;
use std::path::Path;

use ui_test::per_test_config::TestConfig;
use ui_test::{Config, Match, error_on_output_conflict};

fn test_generic(cfg: Config) {
    let stderr_path = Path::new("tests/normalizer.stderr");
    let test_path = Path::new("tests/normalizer.rs");
    let mut text = read_to_string(stderr_path).unwrap();
    assert!(text.contains('/'));
    text = text.replace("/", "\\");
    eprintln!("Stderr: {text:?}\n{text}", text = text);
    let mut errors = Vec::new();
    let test_cfg = TestConfig::one_off_runner(cfg, test_path.to_path_buf());
    error_on_output_conflict(stderr_path, text.as_bytes(), &mut errors, &test_cfg);
    for error in &errors {
        eprintln!("Error: {:?}", error);
        match error {
            ui_test::Error::OutputDiffers { actual, expected, .. } => {
                eprintln!("Actual output: {s:?}\n{s}", s = String::from_utf8_lossy(actual));
                eprintln!("Expected output: {s:?}\n{s}", s = String::from_utf8_lossy(expected));
            },
            _ => {},
        }
    }
    assert!(errors.is_empty(), "Expected no errors, got {} errors", errors.len());
}

#[test]
fn basic() {
    let mut cfg = Config::dummy();
    let normalize_stderr = &mut cfg.comment_defaults.base().normalize_stderr;
    normalize_stderr.push((Match::PathBackslash, b"/".to_vec()));
    test_generic(cfg);
}

#[test]
fn rustc() {
    let cfg = Config::rustc(env!("CARGO_MANIFEST_DIR"));
    test_generic(cfg);
}
