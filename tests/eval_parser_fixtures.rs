use std::fs;
use std::path::Path;

use serde_json::Value;

const FIXTURES_DIR: &str = "tests/fixtures/eval";
const LM_EVAL_SUCCESS_FIXTURE: &str = "tests/fixtures/eval/lm-eval-gsm8k-success.json";
const EVALPLUS_SUCCESS_FIXTURE: &str = "tests/fixtures/eval/evalplus-humaneval-success.jsonl";
const EVALPLUS_FAILURE_FIXTURE: &str = "tests/fixtures/eval/evalplus-subset-eval-failure.txt";

fn read_fixture(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("failed to read {path}: {error}"))
}

#[test]
fn fixture_directory_exists() {
    assert!(Path::new(FIXTURES_DIR).exists());
}

#[test]
fn lm_eval_success_fixture_exposes_primary_metric() {
    let fixture = read_fixture(LM_EVAL_SUCCESS_FIXTURE);
    let json: Value = serde_json::from_str(&fixture).expect("valid lm-eval fixture json");

    assert_eq!(json["results"]["gsm8k"]["name"], "gsm8k");
    assert_eq!(json["results"]["gsm8k"]["exact_match,strict-match"], 1.0);
    assert_eq!(json["model_name"], "gemma-4-E4B-it-UD-Q8_K_XL.gguf");
    assert_eq!(
        json["config"]["model_args"]["base_url"],
        "http://127.0.0.1:8989/v1/completions"
    );
}

#[test]
fn evalplus_success_fixture_exposes_task_and_solution() {
    let fixture = read_fixture(EVALPLUS_SUCCESS_FIXTURE);
    let first_line = fixture.lines().next().expect("evalplus fixture line");
    let json: Value = serde_json::from_str(first_line).expect("valid evalplus fixture json");

    assert_eq!(json["task_id"], "HumanEval/0");
    assert!(json["solution"]
        .as_str()
        .is_some_and(|solution| !solution.is_empty()));
}

#[test]
fn evalplus_failure_fixture_preserves_subset_evaluation_error_shape() {
    let fixture = read_fixture(EVALPLUS_FAILURE_FIXTURE);

    assert!(fixture.contains("AssertionError: Missing problems in samples"));
    assert!(fixture.contains("evalplus.evaluate humaneval"));
}
