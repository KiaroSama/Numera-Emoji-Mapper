//! Import-isolated source generation restores exact module identities on every callback outcome.
use super::source_oracle_runtime::without_applications;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::json;

#[test]
fn application_import_guard_restores_existing_missing_and_none_entries_after_error_and_unwind() {
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let modules = py
            .import("sys")?
            .getattr("modules")?
            .cast_into::<PyDict>()?;
        let names = [
            "emojikit._native",
            "emojikit.catalog",
            "emojikit.operator_config",
            "emojikit.packstate",
            "emojikit.panel_plan",
            "emojikit.panel_view",
            "emojikit.plan_apply",
            "emojikit.collection_state",
        ];
        let previous = names
            .iter()
            .map(|name| modules.get_item(*name))
            .collect::<PyResult<Vec<_>>>()?;
        let sentinel = PyModule::new(py, "guard_fixture")?;
        modules.set_item(names[0], &sentinel)?;
        if modules.contains(names[1])? {
            modules.del_item(names[1])?;
        }
        modules.set_item(names[2], py.None())?;
        let unrelated = "_oracle_guard_unrelated_fixture";
        let unrelated_previous = modules.get_item(unrelated)?;
        let outcomes = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            for outcome in ["success", "error", "panic"] {
                let mut calls = 0;
                let added = PyModule::new(py, "unrelated_import").unwrap();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    without_applications(|| {
                        calls += 1;
                        modules.set_item(unrelated, &added).unwrap();
                        Python::attach(|py| -> PyResult<()> {
                            let current = py
                                .import("sys")?
                                .getattr("modules")?
                                .cast_into::<PyDict>()?;
                            for name in names {
                                assert!(current.get_item(name)?.unwrap().is_none());
                            }
                            Ok(())
                        })
                        .unwrap();
                        match outcome {
                            "error" => Err("source fixture refusal".into()),
                            "panic" => panic!("fixture callback unwind"),
                            _ => Ok(json!({"fixture_drift":false})),
                        }
                    })
                }));
                assert_eq!(calls, 1);
                match outcome {
                    "success" => assert_eq!(
                        result.unwrap().unwrap(),
                        json!({"fixture_drift":false,"blocked_application_modules":8})
                    ),
                    "error" => assert_eq!(result.unwrap().unwrap_err(), "source fixture refusal"),
                    _ => assert!(result.is_err()),
                }
                assert!(modules.get_item(names[0]).unwrap().unwrap().is(&sentinel));
                assert!(!modules.contains(names[1]).unwrap());
                assert!(modules.get_item(names[2]).unwrap().unwrap().is_none());
                assert!(modules.get_item(unrelated).unwrap().unwrap().is(&added));
            }
        }));
        match unrelated_previous {
            Some(value) => modules.set_item(unrelated, value)?,
            None if modules.contains(unrelated)? => modules.del_item(unrelated)?,
            None => {}
        }
        for (name, previous) in names.iter().zip(previous) {
            match previous {
                Some(value) => modules.set_item(*name, value)?,
                None if modules.contains(*name)? => modules.del_item(*name)?,
                None => {}
            }
        }
        if let Err(error) = outcomes {
            std::panic::resume_unwind(error);
        }
        Ok(())
    })
    .unwrap();
}
