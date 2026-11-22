use crate::{
    oracles::{pinned, verify_pins},
    support::*,
};
use numera_runtime as native;
use serde_json::json;
use std::{collections::BTreeMap, path::Path};
// tests.test_native_consumer_boundary.NativeConsumerBoundary.test_converter_watchdog_passes_use_the_installed_native_command
#[test]
fn watchdog_pass_uses_actual_installed_native_converter_and_propagates_partial() {
    let f = Fixture::new();
    let input = f.root.join("in with spaces");
    write(
        &input.join("a.png"),
        std::fs::read(project().join("assets/numera-emoji-mapper-logo.png")).unwrap(),
    );
    write(&input.join("broken.png"), b"corrupt PNG");
    let output = f.root.join("out");
    let stdout = f.root.join("stdout");
    let stderr = f.root.join("stderr");
    let args = vec![
        "make-emoji-pngs".into(),
        "--in".into(),
        input.display().to_string(),
        "--out".into(),
        output.display().to_string(),
    ];
    let plan = native::tooling_launch::plan(&f.root, &args, &std::env::vars().collect()).unwrap();
    assert_eq!(
        plan.argv[1..],
        args.iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        native::package::digest(Path::new(&plan.argv[0])).unwrap(),
        native::package::digest(&f.binary).unwrap()
    );
    let r = f.run(
        &[
            "native-convert-owner",
            "--in",
            input.to_str().unwrap(),
            "--out",
            output.to_str().unwrap(),
            "--stdout",
            stdout.to_str().unwrap(),
            "--stderr",
            stderr.to_str().unwrap(),
        ],
        &[],
    );
    r.code(3);
    let out = std::fs::read_to_string(stdout).unwrap();
    assert!(out.contains("DONE: made 1 this run"));
    assert!(out.contains("failed=1"));
    assert!(output.join("a.png").is_file());
    assert!(!output.join("broken.png").exists());
    let owner = std::fs::read_to_string(project().join("runtime/src/tooling_convert.rs")).unwrap();
    assert!(!owner.contains("emojikit.make_emoji_pngs"));
    assert!(!owner.contains("emojikit\\make_emoji_pngs.py"));
}
fn production_modules() -> BTreeMap<String, std::path::PathBuf> {
    let path = project().join("runtime/src/lib.rs");
    let file = syn::parse_file(&std::fs::read_to_string(&path).unwrap()).unwrap();
    file.items
        .into_iter()
        .filter_map(|item| {
            if let syn::Item::Mod(module) = item {
                if module.attrs.iter().any(|a| {
                    a.path().is_ident("cfg")
                        && a.meta
                            .require_list()
                            .is_ok_and(|l| l.tokens.to_string().contains("test"))
                }) {
                    return None;
                }
                let source = module
                    .attrs
                    .iter()
                    .find_map(|a| {
                        if !a.path().is_ident("path") {
                            return None;
                        }
                        if let syn::Meta::NameValue(meta) = &a.meta
                            && let syn::Expr::Lit(expr) = &meta.value
                            && let syn::Lit::Str(path) = &expr.lit
                        {
                            Some(Path::new("runtime/src").join(path.value()))
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| {
                        let flat = Path::new("runtime/src").join(format!("{}.rs", module.ident));
                        if project().join(&flat).is_file() {
                            flat
                        } else {
                            Path::new("runtime/src")
                                .join(module.ident.to_string())
                                .join("mod.rs")
                        }
                    });
                Some((module.ident.to_string(), project().join(source)))
            } else {
                None
            }
        })
        .collect()
}
// tests.test_native_consumer_boundary.NativeConsumerBoundary.test_relocated_references_preserve_source_behavior_and_pinned_oracles
#[test]
fn relocated_rust_business_modules_and_pinned_oracles_preserve_boundaries() {
    verify_pins(&project(), pinned()).unwrap();
    assert_eq!(
        pinned()
            .lines()
            .filter(|l| l.contains("tests/oracles/"))
            .count(),
        20
    );
    let modules = production_modules();
    for (name, path) in &modules {
        assert!(path.is_file(), "{name}: {}", path.display());
        syn::parse_file(&std::fs::read_to_string(path).unwrap()).unwrap();
    }
    for name in [
        "catalog",
        "intent",
        "resume",
        "plan_apply",
        "plan_status",
        "panel_view",
        "migration_apply",
        "migration_bundle",
        "migration_restore",
        "collector",
        "publication",
        "publish_collection",
        "roster",
        "roster_index",
        "archive_rows",
        "pack_gallery",
        "pack_export",
        "telegram",
        "telegram_mutation",
        "ticker_state",
        "ticker_build",
        "ingest",
        "codec_image",
        "codec_video",
        "codec_worker",
        "config",
        "locks",
        "ownership",
        "tooling_sandbox",
        "tooling_convert",
    ] {
        assert!(
            modules.contains_key(name),
            "missing relocated business module {name}"
        );
    }
    assert!(modules.len() >= 30);
}
// tests.test_native_consumer_boundary.NativeConsumerBoundary.test_all_references_import_with_retired_production_modules_blocked
#[test]
fn standalone_business_graph_runs_with_retired_interpreter_modules_absent() {
    let f = Fixture::new();
    assert!(!f.root.join("emojikit").exists());
    assert!(!f.root.join("tests/reference").exists());
    let path = f.root.join("source.png");
    write(
        &path,
        std::fs::read(project().join(
            "runtime/tests/baseline_cli_status_support/fixtures/numera-emoji-mapper-logo.png",
        ))
        .unwrap(),
    );
    let r = f.run(
        &["add-media", path.to_str().unwrap(), "--as", "static"],
        &[
            (
                "PYTHONPATH",
                f.root.join("absent-python").display().to_string(),
            ),
            (
                "PYO3_PYTHON",
                f.root.join("must-not-run-python").display().to_string(),
            ),
        ],
    );
    r.code(0);
    assert!(r.out.contains("new=1 dedup=0 failed=0"));
    assert_eq!(f.catalog().all(None).unwrap().len(), 1);
    let modules = production_modules();
    assert!(modules.len() >= 30);
    for path in modules.values() {
        let text = std::fs::read_to_string(path).unwrap();
        let file = syn::parse_file(&text).unwrap();
        for item in file.items {
            match item {
                syn::Item::ExternCrate(item) => assert_ne!(item.ident, "pyo3"),
                syn::Item::Use(item) => assert!(!use_mentions(&item.tree, "pyo3")),
                _ => {}
            }
        }
    }
    assert_eq!([220_u8, 20, 20, 255].repeat(100 * 100).len(), 100 * 100 * 4);
}
// tests.test_native_consumer_boundary.NativeConsumerBoundary.test_sandbox_imports_without_the_python_panel_application
#[test]
fn sandbox_module_runs_without_panel_application_or_reference_import() {
    assert_eq!(native::panel_session::DEFAULT_PORT, 9450);
    let f = Fixture::new();
    {
        let mut cat = f.catalog();
        let path = f.data().join("media/item.png");
        write(&path, b"owned bytes do not need decoding to snapshot");
        cat.insert(
            &json!({"content_key":"s:fixture","fmt":"static","file_path":path}),
            UTC,
        )
        .unwrap();
    }
    let target = f.root.join("panel-sandbox-consumer");
    let session = native::tooling_sandbox::Session::create(&f.root, &f.data(), &target).unwrap();
    assert!(target.join("catalog.db").is_file());
    assert!(native::tooling_sandbox::marker_is_self_describing(&target));
    session.finish().unwrap();
    assert!(!target.exists());
    let source = std::fs::read_to_string(project().join("runtime/src/tooling_sandbox.rs")).unwrap();
    let file = syn::parse_file(&source).unwrap();
    for item in file.items {
        if let syn::Item::Use(item) = item {
            assert!(!use_mentions(&item.tree, "reference"));
            assert!(!use_mentions(&item.tree, "panel_http"));
        }
    }
    assert!(!source.contains("crate::panel_http"));
    assert!(!source.contains("tests/reference"));
}
fn use_mentions(tree: &syn::UseTree, name: &str) -> bool {
    match tree {
        syn::UseTree::Path(p) => p.ident == name || use_mentions(&p.tree, name),
        syn::UseTree::Name(n) => n.ident == name,
        syn::UseTree::Rename(n) => n.ident == name,
        syn::UseTree::Group(g) => g.items.iter().any(|t| use_mentions(t, name)),
        syn::UseTree::Glob(_) => false,
    }
}
