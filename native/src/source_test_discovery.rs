//! Static source collection: Rust C3/member ownership, with unresolved dynamic behavior kept explicit.
use crate::source_test_inventory::{body, kind, source_path};
use pyo3::prelude::*;
use pyo3::types::PyList;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
enum Member {
    Callable(u64),
    Noncallable,
    Unknown,
}
#[derive(Clone)]
pub(crate) struct Class {
    name: String,
    identity: String,
    bases: Vec<String>,
    members: BTreeMap<String, Member>,
}
#[derive(Clone)]
pub(crate) struct Model {
    pub(crate) classes: Vec<Class>,
    pub(crate) unresolved: Vec<String>,
    pub(crate) bindings: BTreeMap<String, String>,
}

type ImportResolver<'a> = dyn FnMut(&str, Option<&str>, usize) -> PyResult<Option<String>> + 'a;

fn binding(
    expression: &str,
    aliases: &BTreeMap<String, String>,
    imports: &mut ImportResolver<'_>,
) -> PyResult<Option<String>> {
    let mut parts = expression.split('.');
    let mut value = aliases.get(parts.next().unwrap_or("")).cloned();
    for part in parts {
        value = match value.as_deref() {
            Some("$unittest") if part == "TestCase" => Some("$unittest.TestCase".into()),
            Some(value) if value.starts_with("$module:") => imports(&value[8..], Some(part), 0)?,
            _ => None,
        };
    }
    Ok(value)
}

fn definition_effects(node: &Bound<'_, PyAny>, postponed: bool) -> PyResult<bool> {
    let args = node.getattr("args")?;
    for field in ["defaults", "kw_defaults"] {
        for value in args.getattr(field)?.cast_into::<PyList>()?.iter() {
            if !value.is_none() && kind(&value)? != "Constant" {
                return Ok(true);
            }
        }
    }
    if !postponed {
        let returns = node.getattr("returns")?;
        if !returns.is_none() && kind(&returns)? != "Constant" {
            return Ok(true);
        }
        let mut parameters = Vec::new();
        for field in ["posonlyargs", "args", "kwonlyargs"] {
            parameters.extend(args.getattr(field)?.cast_into::<PyList>()?.iter());
        }
        for field in ["vararg", "kwarg"] {
            let parameter = args.getattr(field)?;
            if !parameter.is_none() {
                parameters.push(parameter);
            }
        }
        for parameter in parameters {
            let annotation = parameter.getattr("annotation")?;
            if !annotation.is_none() && kind(&annotation)? != "Constant" {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub(crate) fn model(
    py: Python<'_>,
    path: &str,
    source: &str,
    imports: &mut ImportResolver<'_>,
) -> PyResult<Model> {
    let ast = py.import("ast")?;
    let tree = ast.call_method1("parse", (source, path))?;
    let mut aliases = BTreeMap::from([("object".to_owned(), "$object".to_owned())]);
    let mut classes = Vec::new();
    let mut unresolved = Vec::new();
    let mut postponed = false;
    for node in body(&tree)? {
        match kind(&node)?.as_str() {
            "Import" | "ImportFrom" => {
                let from = kind(&node)? == "ImportFrom";
                let module = if from {
                    node.getattr("module")?.extract::<Option<String>>()?
                } else {
                    None
                };
                let absolute = !from || node.getattr("level")?.extract::<u64>()? == 0;
                for alias in node.getattr("names")?.cast_into::<PyList>()?.iter() {
                    let name = alias.getattr("name")?.extract::<String>()?;
                    if absolute
                        && from
                        && module.as_deref() == Some("__future__")
                        && name == "annotations"
                    {
                        postponed = true;
                        continue;
                    }
                    let local = alias
                        .getattr("asname")?
                        .extract::<Option<String>>()?
                        .unwrap_or_else(|| {
                            if from {
                                name.clone()
                            } else {
                                name.split('.').next().unwrap_or(&name).into()
                            }
                        });
                    aliases.remove(&local);
                    if classes.iter().any(|class: &Class| class.name == local) {
                        unresolved.push(format!("{local}: imported name replaces a local class"));
                    }
                    if absolute
                        && ((from && module.as_deref() == Some("unittest") && name == "TestCase")
                            || (!from && name == "unittest"))
                    {
                        aliases.insert(
                            local,
                            if from {
                                "$unittest.TestCase".into()
                            } else {
                                "$unittest".into()
                            },
                        );
                    } else {
                        // A syntax-resolved candidate is not proof an import executed successfully.
                        unresolved.push(format!("{local}: unresolved imported module binding"));
                        let value = if from {
                            imports(
                                module.as_deref().unwrap_or(""),
                                Some(&name),
                                node.getattr("level")?.extract::<usize>()?,
                            )?
                        } else {
                            let module = if alias.getattr("asname")?.is_none() {
                                name.split('.').next().unwrap_or(&name)
                            } else {
                                &name
                            };
                            imports(module, None, 0)?
                        };
                        if let Some(value) = value {
                            aliases.insert(local, value);
                        }
                    }
                }
            }
            "ClassDef" => {
                let name = node.getattr("name")?.extract::<String>()?;
                if !node
                    .getattr("decorator_list")?
                    .cast_into::<PyList>()?
                    .is_empty()
                    || !node.getattr("keywords")?.cast_into::<PyList>()?.is_empty()
                {
                    unresolved.push(format!("{name}: class decorator/metaclass"));
                }
                let mut bases = Vec::new();
                for base in node.getattr("bases")?.cast_into::<PyList>()?.iter() {
                    let expression: String = ast.call_method1("unparse", (&base,))?.extract()?;
                    let resolved = binding(&expression, &aliases, imports)?
                        .unwrap_or_else(|| format!("?{expression}"));
                    if resolved.starts_with('?') {
                        unresolved.push(format!("{name}: unresolved base {expression}"));
                    }
                    bases.push(resolved);
                }
                if bases.is_empty() {
                    bases.push("$object".into());
                }
                let mut members = BTreeMap::new();
                for child in body(&node)? {
                    match kind(&child)?.as_str() {
                        "FunctionDef" | "AsyncFunctionDef" => {
                            let method = child.getattr("name")?.extract::<String>()?;
                            if method == "__init_subclass__"
                                || definition_effects(&child, postponed)?
                            {
                                unresolved.push(format!(
                                    "{name}.{method}: definition-time class effects"
                                ));
                            }
                            let decorated = !child
                                .getattr("decorator_list")?
                                .cast_into::<PyList>()?
                                .is_empty();
                            if decorated {
                                unresolved.push(format!("{name}.{method}: decorated callable"));
                            }
                            members.insert(
                                method,
                                if decorated {
                                    Member::Unknown
                                } else {
                                    Member::Callable(child.getattr("lineno")?.extract::<u64>()?)
                                },
                            );
                        }
                        "Assign" | "AnnAssign" => {
                            if kind(&child)? == "AnnAssign"
                                && !postponed
                                && kind(&child.getattr("annotation")?)? != "Constant"
                            {
                                unresolved
                                    .push(format!("{name}: definition-time annotation effects"));
                            }
                            let targets = if kind(&child)? == "Assign" {
                                child
                                    .getattr("targets")?
                                    .cast_into::<PyList>()?
                                    .iter()
                                    .collect::<Vec<_>>()
                            } else {
                                vec![child.getattr("target")?]
                            };
                            let value = child.getattr("value")?;
                            // An annotation without a value does not mask an inherited member.
                            if value.is_none() {
                                continue;
                            }
                            let known = kind(&value)? == "Constant";
                            for target in targets {
                                if kind(&target)? != "Name" {
                                    unresolved.push(format!("{name}: complex class assignment"));
                                    continue;
                                }
                                let member = target.getattr("id")?.extract::<String>()?;
                                if member == "__slots__" {
                                    // Slots create descriptors that can shadow inherited test methods.
                                    unresolved.push(format!("{name}: slot-definition effects"));
                                }
                                if !known {
                                    unresolved
                                        .push(format!("{name}.{member}: dynamic class value"));
                                }
                                members.insert(
                                    member,
                                    if known {
                                        Member::Noncallable
                                    } else {
                                        Member::Unknown
                                    },
                                );
                            }
                        }
                        "Pass" => {}
                        "Expr" if kind(&child.getattr("value")?)? == "Constant" => {}
                        _ => unresolved.push(format!("{name}: conditional/dynamic class body")),
                    }
                }
                let identity = format!("{path}:{}", node.getattr("lineno")?.extract::<u64>()?);
                aliases.insert(name.clone(), identity.clone());
                classes.push(Class {
                    name,
                    identity,
                    bases,
                    members,
                });
                if classes.len() > 2048 {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "source discovery exceeds class limit",
                    ));
                }
            }
            "FunctionDef" | "AsyncFunctionDef" => {
                let name = node.getattr("name")?.extract::<String>()?;
                aliases.remove(&name);
                if !node
                    .getattr("decorator_list")?
                    .cast_into::<PyList>()?
                    .is_empty()
                    || definition_effects(&node, postponed)?
                    || matches!(name.as_str(), "__dir__" | "__getattr__")
                {
                    unresolved.push(format!("{name}: definition-time/module collection effects"));
                }
                if name == "load_tests" {
                    unresolved.push("module load_tests custom collection".into());
                }
            }
            "Pass" => {}
            "Raise" => unresolved.push("module raises before runtime collection".into()),
            "Expr" if kind(&node.getattr("value")?)? == "Constant" => {}
            _ => unresolved.push("module conditional/dynamic definitions".into()),
        }
    }
    Ok(Model {
        classes,
        unresolved,
        bindings: aliases,
    })
}

fn linearize(
    name: &str,
    classes: &[Class],
    visiting: &mut BTreeSet<String>,
    cache: &mut BTreeMap<String, Vec<String>>,
) -> Result<Vec<String>, String> {
    if name == "$object" {
        return Ok(vec![name.into()]);
    }
    if name == "$unittest.TestCase" {
        return Ok(vec![name.into(), "$object".into()]);
    }
    if let Some(mro) = cache.get(name) {
        return Ok(mro.clone());
    }
    if visiting.len() >= 128 || !visiting.insert(name.into()) {
        return Err("TypeError: cyclic or excessive source inheritance".into());
    }
    let class = classes
        .iter()
        .find(|class| class.identity == name)
        .ok_or_else(|| format!("unresolved source base {name}"))?;
    if class.bases.iter().collect::<BTreeSet<_>>().len() != class.bases.len() {
        return Err(format!("TypeError: duplicate base class in {name}"));
    }
    let mut sequences = class
        .bases
        .iter()
        .map(|base| linearize(base, classes, visiting, cache))
        .collect::<Result<Vec<_>, _>>()?;
    sequences.push(class.bases.clone());
    let mut result = vec![name.into()];
    while sequences.iter().any(|sequence| !sequence.is_empty()) {
        let candidate = sequences
            .iter()
            .filter_map(|sequence| sequence.first())
            .find(|head| {
                sequences
                    .iter()
                    .all(|sequence| !sequence.iter().skip(1).any(|value| value == *head))
            })
            .cloned()
            .ok_or_else(|| format!("TypeError: inconsistent method resolution order for {name}"))?;
        result.push(candidate.clone());
        for sequence in &mut sequences {
            if sequence.first() == Some(&candidate) {
                sequence.remove(0);
            }
        }
    }
    visiting.remove(name);
    cache.insert(name.into(), result.clone());
    Ok(result)
}

pub fn discover(path: &str, source: &str) -> Result<Value, String> {
    source_path(path)?;
    if source.len() > 2 * 1024 * 1024 {
        return Err("source discovery input exceeds 2MiB".into());
    }
    Python::initialize();
    let model = Python::attach(|py| model(py, path, source, &mut |_, _, _| Ok(None)))
        .map_err(|error| format!("source discovery parse failed: {error}"))?;
    collect(&model, &model.classes)
}

pub(crate) fn collect(model: &Model, classes: &[Class]) -> Result<Value, String> {
    let mut unresolved = model.unresolved.clone();
    let bindings = &model.bindings;
    let mut cases = Vec::new();
    let mut origins = Vec::new();
    let mut cache = BTreeMap::new();
    // Every class must be constructible, including one later hidden by a module rebind.
    for class in &model.classes {
        match linearize(&class.identity, classes, &mut BTreeSet::new(), &mut cache) {
            Ok(_) => {}
            Err(error) if error.starts_with("unresolved") => unresolved.push(error),
            Err(error) => return Err(error),
        }
    }
    // The loader visits sorted final module bindings; bases keep their definition-time identities.
    for (binding, identity) in bindings {
        let Some(class) = classes.iter().find(|class| &class.identity == identity) else {
            continue;
        };
        let mro = match linearize(&class.identity, classes, &mut BTreeSet::new(), &mut cache) {
            Ok(mro) => mro,
            Err(error) if error.starts_with("unresolved") => {
                unresolved.push(error);
                continue;
            }
            Err(error) => return Err(error),
        };
        if !mro.iter().any(|name| name == "$unittest.TestCase") {
            continue;
        }
        let mut members = BTreeMap::new();
        for owner in &mro {
            if let Some(definition) = classes
                .iter()
                .find(|definition| &definition.identity == owner)
            {
                for (method, member) in &definition.members {
                    members
                        .entry(method.clone())
                        .or_insert((definition, member));
                }
            }
        }
        let mut found = false;
        for (method, (owner, member)) in &members {
            if method.starts_with("test")
                && let Member::Callable(line) = member
            {
                cases.push(json!({"class":class.name,"method":method,"defined_in":owner.name}));
                origins.push(json!({"binding":binding,"class_identity":class.identity,"method":method,"defined_in_identity":owner.identity,"method_line":line}));
                found = true;
            }
        }
        if !found && let Some((owner, member)) = members.get("runTest") {
            let line = match member {
                Member::Callable(line) => Some(*line),
                _ => None,
            };
            cases.push(json!({"class":class.name,"method":"runTest","defined_in":owner.name}));
            origins.push(json!({"binding":binding,"class_identity":class.identity,"method":"runTest","defined_in_identity":owner.identity,"method_line":line}));
        }
    }
    unresolved.sort();
    unresolved.dedup();
    Ok(
        json!({"cases":cases,"case_origins":origins,"discovery_complete":unresolved.is_empty(),"unresolved":unresolved}),
    )
}
