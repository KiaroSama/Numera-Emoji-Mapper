//! Supplemental native codec, ownership and frozen-input regressions.
extern crate numera_runtime as _native;
#[path = "supplemental_native_support/corpus_guards.rs"]
mod corpus_guards;
#[path = "supplemental_native_support/corpus_recipe.rs"]
mod corpus_recipe;
#[path = "supplemental_native_support/fixture.rs"]
mod fixture;
#[path = "supplemental_native_support/codec_lottie_identity.rs"]
mod lottie_identity;
#[path = "supplemental_native_support/codec_lottie_preview.rs"]
mod lottie_preview;
#[path = "supplemental_native_support/lottie_recipe.rs"]
mod lottie_recipe;
#[path = "supplemental_native_support/codec_lottie_validate.rs"]
mod lottie_validation;
#[path = "supplemental_native_support/media_native_lifecycle.rs"]
mod native_lifecycle;
#[path = "supplemental_native_support/process_liveness.rs"]
mod process_liveness;
#[path = "supplemental_native_support/codec_process_policy.rs"]
mod process_policy;
#[path = "support/test_admission.rs"]
mod test_admission;
#[path = "supplemental_native_support/codec_worker_error_ipc.rs"]
mod worker_ipc;
#[path = "supplemental_native_support/codec_worker_source_parity.rs"]
mod worker_source;
