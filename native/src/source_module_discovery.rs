//! Resolve static class candidates across an admitted corpus without importing repository modules.
use crate::source_git_inventory::Source;
use crate::source_test_discovery::{Model, collect, model};
use pyo3::prelude::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

struct Corpus<'a> {
    sources: BTreeMap<String, &'a Source>,
    models: BTreeMap<String, Model>,
    loading: BTreeSet<String>,
}

fn module_name(path: &str) -> String {
    path.strip_suffix("/__init__.py")
        .unwrap_or_else(|| path.strip_suffix(".py").unwrap_or(path))
        .replace('/', ".")
}

impl Corpus<'_> {
    fn exists(&self, name: &str) -> bool {
        self.sources.contains_key(name)
            || self.sources.keys().any(|module| {
                module
                    .strip_prefix(name)
                    .is_some_and(|tail| tail.starts_with('.'))
            })
    }

    fn absolute(&self, caller: &str, name: &str, level: usize) -> Option<String> {
        if level == 0 {
            return (!name.is_empty()).then(|| name.to_owned());
        }
        let source = self.sources.get(caller)?;
        let package = if source.path.ends_with("/__init__.py") {
            caller
        } else {
            caller.rsplit_once('.').map_or("", |(package, _)| package)
        };
        let mut parts = package.split('.').collect::<Vec<_>>();
        if package.is_empty() || level > parts.len() {
            return None;
        }
        parts.truncate(parts.len() - (level - 1));
        let base = parts.join(".");
        Some(if name.is_empty() {
            base
        } else {
            format!("{base}.{name}")
        })
    }

    fn resolve(
        &mut self,
        py: Python<'_>,
        caller: &str,
        module: &str,
        symbol: Option<&str>,
        level: usize,
    ) -> PyResult<Option<String>> {
        let Some(module) = self.absolute(caller, module, level) else {
            return Ok(None);
        };
        if !self.exists(&module) {
            return Ok(None);
        }
        let definition = self.load(py, &module)?;
        if let Some(symbol) = symbol {
            if let Some(value) = definition.and_then(|model| model.bindings.get(symbol).cloned()) {
                return Ok(Some(value));
            }
            let child = format!("{module}.{symbol}");
            if self.exists(&child) {
                self.load(py, &child)?;
                return Ok(Some(format!("$module:{child}")));
            }
            Ok(None)
        } else {
            Ok(Some(format!("$module:{module}")))
        }
    }

    fn load(&mut self, py: Python<'_>, module: &str) -> PyResult<Option<Model>> {
        if let Some(model) = self.models.get(module) {
            return Ok(Some(model.clone()));
        }
        let Some(source) = self.sources.get(module).copied() else {
            return Ok(None);
        };
        // Partial cyclic imports cannot justify complete collection; the caller retains its
        // unresolved import/base rather than executing code to guess the initialization order.
        if self.loading.len() >= 128 || !self.loading.insert(module.into()) {
            return Ok(None);
        }
        let result = model(
            py,
            &source.path,
            &source.text,
            &mut |name, symbol, level| self.resolve(py, module, name, symbol, level),
        );
        self.loading.remove(module);
        let model = result?;
        self.models.insert(module.into(), model.clone());
        Ok(Some(model))
    }
}

pub fn discover(sources: &[Source]) -> Result<Value, String> {
    if sources.is_empty() || sources.len() > 4096 {
        return Err("source discovery requires1..4096files".into());
    }
    let baseline = &sources[0].baseline_revision;
    if baseline.len() != 40
        || !baseline
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("source discovery requires an exact lowercase40hex revision".into());
    }
    let mut modules = BTreeMap::new();
    let mut bytes = 0usize;
    for source in sources {
        crate::source_test_inventory::source_path(&source.path)?;
        bytes = bytes
            .checked_add(source.text.len())
            .ok_or("source corpus byte limit")?;
        if source.text.len() > 2 * 1024 * 1024 || bytes > 32 * 1024 * 1024 {
            return Err("source discovery exceeds corpus byte limit".into());
        }
        let hash = Sha256::digest(source.text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if source.baseline_revision != *baseline || source.sha256 != hash {
            return Err("source discovery corpus identity drift".into());
        }
        if modules.insert(module_name(&source.path), source).is_some() {
            return Err("source discovery has duplicate or ambiguous modules".into());
        }
    }
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        let mut corpus = Corpus {
            sources: modules,
            models: BTreeMap::new(),
            loading: BTreeSet::new(),
        };
        let names = corpus.sources.keys().cloned().collect::<Vec<_>>();
        for module in &names {
            corpus
                .load(py, module)
                .map_err(|error| format!("source module parse failed: {error}"))?;
        }
        let classes = corpus
            .models
            .values()
            .flat_map(|model| model.classes.iter().cloned())
            .collect::<Vec<_>>();
        let mut found = serde_json::Map::new();
        for module in names {
            let model = corpus
                .models
                .get(&module)
                .ok_or("source module model is missing")?;
            let value = collect(model, &classes)?;
            found.insert(corpus.sources[&module].path.clone(), value);
        }
        Ok(json!(found))
    })
}
