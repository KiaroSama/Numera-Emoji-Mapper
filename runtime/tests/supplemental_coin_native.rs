//! Retained native product scenarios; supplemental to original1516.
extern crate numera_runtime as _native;
#[path = "supplemental_coin_native_support/mod.rs"]
mod support;
pub use support::bot_api;
#[path = "support/test_admission.rs"]
mod test_admission;
