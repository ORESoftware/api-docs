//! Execute the same native test bodies in a real browser, not a parallel port.
#![cfg(target_arch = "wasm32")]
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

mod conformance {
    include!("conformance.rs");
    pub fn run() {
        shared_corpus();
        invalid_configuration_never_becomes_a_permissive_validator();
        input_budget_and_numeric_overflow();
        serde_and_garde_compose_without_treating_decode_as_validation();
        edit_blur_submit_revalidates_and_clears_old_errors();
    }
}

mod boundaries {
    include!("boundaries.rs");
    pub fn run() {
        email_mailbox_and_label_limits();
        separators_and_scalar_budgets_are_exact();
        calendar_century_and_integer_boundaries();
        submission_revalidates_values_changed_without_an_edit_event();
    }
}

#[wasm_bindgen_test]
fn shared_corpus_and_lifecycle_in_browser() {
    conformance::run();
}

#[wasm_bindgen_test]
fn boundary_and_submission_regressions_in_browser() {
    boundaries::run();
}
