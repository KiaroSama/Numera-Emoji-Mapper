//! The original main Rust suite is intentionally separate from the1516 Python identities.
use crate::{
    Target,
    ast::{Function, Inventory, digest},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const REVISION: &str = "9c3cbb43305b0e507085003d4a33806e4a60b677";
pub const SOURCE_SHA256: &str = "03c2ed4a3077b7b588d1be77cc45bcf8456af8218cfb77e67cfcef440752882d";
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCase {
    pub id: String,
    pub source_file: String,
    pub source_sha256: String,
    pub linux: bool,
    pub windows: bool,
    pub technical_helper: bool,
    pub disposition: String,
    pub target: Option<Target>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub source_revision: String,
    pub cases: Vec<NativeCase>,
    pub fixture_sha256: BTreeMap<String, String>,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub declarations: usize,
    pub applicable: usize,
    pub product_behaviors: usize,
    pub technical_helpers: usize,
    pub retired_macos: usize,
    pub linux_original_declarations: usize,
    pub windows_original_declarations: usize,
    pub errors: Vec<String>,
    pub counts_toward_python_1516: bool,
    pub proves_execution: bool,
}
pub fn source_digest(manifest: &Manifest) -> String {
    let mut rows = manifest
        .cases
        .iter()
        .map(|c| {
            format!(
                "{}\t{}\t{}\t{}\t{}\t{}\n",
                c.id, c.source_file, c.source_sha256, c.linux, c.windows, c.technical_helper
            )
        })
        .collect::<Vec<_>>();
    rows.extend(
        manifest
            .fixture_sha256
            .iter()
            .map(|(path, sha)| format!("fixture\t{path}\t{sha}\n")),
    );
    rows.sort();
    digest(rows.join("").as_bytes())
}
fn functions<'a>(c: &NativeCase, i: &'a Inventory) -> Vec<&'a Function> {
    c.target
        .as_ref()
        .map(|t| {
            i.functions
                .iter()
                .filter(|f| f.file == t.file && f.target == t.target && f.function == t.function)
                .collect()
        })
        .unwrap_or_default()
}
pub fn refresh(manifest: &mut Manifest, inventory: &Inventory) {
    for c in &mut manifest.cases {
        if let [function] = functions(c, inventory).as_slice() {
            c.target.as_mut().unwrap().body_sha256 = Some(function.body_sha256.clone());
        }
    }
}
pub fn verify(manifest: &Manifest, inventory: &Inventory) -> Report {
    let mut errors = Vec::new();
    if manifest.schema != 1
        || manifest.source_revision != REVISION
        || source_digest(manifest) != SOURCE_SHA256
    {
        errors.push("original-main Rust source receipt changed".into());
    }
    if manifest.cases.len() != 35
        || manifest
            .cases
            .iter()
            .map(|c| &c.id)
            .collect::<BTreeSet<_>>()
            .len()
            != 35
    {
        errors.push("original-main Rust requires35 unique declarations, including the explicit retired macOS case".into());
    }
    let mut applicable = 0;
    let mut helpers = 0;
    let mut retired = 0;
    let mut occupied = BTreeSet::new();
    for c in &manifest.cases {
        if !c.linux && !c.windows {
            retired += 1;
            if c.disposition != "retired-macos" || c.target.is_some() || c.technical_helper {
                errors.push(format!(
                    "{} is not the explicit unsupported macOS disposition",
                    c.id
                ));
            }
            continue;
        }
        applicable += 1;
        helpers += usize::from(c.technical_helper);
        let matches = functions(c, inventory);
        let [function] = matches.as_slice() else {
            errors.push(format!(
                "original-main {} maps to{} actual functions",
                c.id,
                matches.len()
            ));
            continue;
        };
        if !occupied.insert((&function.file, &function.target, &function.function)) {
            errors.push(format!(
                "original-main {} shares a target with another original-main declaration",
                c.id
            ));
        }
        if function.linux != c.linux || function.windows != c.windows {
            errors.push(format!(
                "original-main {} platform gates differ from source",
                c.id
            ));
        }
        if c.target.as_ref().unwrap().body_sha256.as_deref() != Some(function.body_sha256.as_str())
        {
            errors.push(format!(
                "original-main {} body changed since disposition review",
                c.id
            ));
        }
        if !function.nonempty {
            errors.push(format!("original-main {} has empty body", c.id));
        }
    }
    if applicable != 34 || helpers != 2 || retired != 1 {
        errors.push(format!("original-main requires34applicable declarations (32behaviors+2helpers) plus1retired macOS; found{applicable}/{helpers}/{retired}"));
    }
    Report {
        declarations: manifest.cases.len(),
        applicable,
        product_behaviors: applicable.saturating_sub(helpers),
        technical_helpers: helpers,
        retired_macos: retired,
        linux_original_declarations: manifest.cases.iter().filter(|c| c.linux).count(),
        windows_original_declarations: manifest.cases.iter().filter(|c| c.windows).count(),
        errors,
        counts_toward_python_1516: false,
        proves_execution: false,
    }
}

pub fn verify_fixtures(manifest: &Manifest, root: &std::path::Path) -> Vec<String> {
    let mut errors = Vec::new();
    let Ok(allowed) = root.join("tests/fixtures").canonicalize() else {
        return vec!["original-main fixture root is absent".into()];
    };
    for (file, expected) in &manifest.fixture_sha256 {
        let path = root.join(file);
        match path.canonicalize() {
            Ok(path) if path.starts_with(&allowed) => match std::fs::read(path) {
                Ok(bytes) if digest(&bytes) == *expected => {}
                _ => errors.push(format!("original-main frozen fixture hash changed: {file}")),
            },
            _ => errors.push(format!(
                "original-main frozen fixture is absent or outside public fixture root: {file}"
            )),
        }
    }
    errors
}
