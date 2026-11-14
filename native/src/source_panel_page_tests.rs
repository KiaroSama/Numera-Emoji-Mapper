//! Unchanged served-text assertions share the admitted panel assets and existing historical result.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::prelude::*;
use pyo3::types::PyDict;

pub(crate) fn run<'py>(
    py: Python<'py>,
    sources: &[&Source],
    bindings: &Bound<'py, PyDict>,
    result: &Bound<'py, PyAny>,
) -> PyResult<Vec<String>> {
    let panel = required(bindings, "p")?;
    panel.setattr(
        "PER_SET",
        crate::source_logging_oracle::literal(py, sources[19], "PER_SET", false)?,
    )?;
    let page = PyDict::new(py);
    page.set_item("p", &panel)?;
    page.set_item("PAGE", panel.getattr("PAGE")?)?;
    page.set_item("SCRIPT", panel.getattr("SCRIPT")?)?;
    page.set_item("ROOT", required(bindings, "ROOT")?)?;
    page.set_item("unittest", py.import("unittest")?)?;
    page.set_item("re", py.import("re")?)?;
    let helper = crate::source_oracle_runtime::namespace(py, sources[18], &["block"], &[], &page)?;
    page.set_item("block", required(&helper, "block")?)?;
    let loader = py.import("unittest")?.getattr("TestLoader")?.call0()?;
    let groups: [(&Source, &[(&str, usize)]); 2] = [
        (
            sources[16],
            &[
                ("DragAndDropOrdering", 11),
                ("UndoRedoAndFormatColours", 11),
                ("SelectionModeCarriesARun", 12),
            ],
        ),
        (
            sources[17],
            &[
                ("OffScreenCostsNothing", 4),
                ("TheGridIsVirtual", 5),
                ("ZoomFitsMoreOrLess", 6),
                ("ThePanelPageActuallyShips", 2),
                ("PackSplitsAndJumpButtons", 8),
            ],
        ),
    ];
    let mut cases = Vec::new();
    for (source, classes) in groups {
        let names = classes.iter().map(|(name, _)| *name).collect::<Vec<_>>();
        let tests = crate::source_oracle_runtime::namespace(
            py,
            source,
            &names,
            &["__build_class__", "len", "set", "chr", "str"],
            &page,
        )?;
        for (name, count) in classes {
            let class = required(&tests, name)?;
            let methods: Vec<String> = loader
                .call_method1("getTestCaseNames", (&class,))?
                .extract()?;
            if methods.len() != *count {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "original panel page collection drift",
                ));
            }
            for method in methods {
                class.call1((&method,))?.call_method1("run", (result,))?;
                for field in [
                    "errors",
                    "failures",
                    "skipped",
                    "expectedFailures",
                    "unexpectedSuccesses",
                ] {
                    if result.getattr(field)?.len()? != 0 {
                        return Err(pyo3::exceptions::PyValueError::new_err(format!(
                            "original panel page {name}.{method} reported {field}; retained"
                        )));
                    }
                }
                cases.push(format!("{name}.{method}"));
            }
        }
    }
    Ok(cases)
}
