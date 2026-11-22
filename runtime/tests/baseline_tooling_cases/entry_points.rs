use super::*;
// tests.test_entry_points.EntryPointsImport.test_discovery_found_the_expected_tools
#[test]
fn test_discovery_found_the_expected_tools() {
    let commands = commands();
    assert!(commands.len() >= 15);
    for command in [
        "build-pack",
        "emoji-bot",
        "panel",
        "coin-verify-logos",
        "coin-rebuild",
    ] {
        assert!(commands.contains(command));
    }
    assert_eq!(
        std::fs::read_dir(project())
            .unwrap()
            .filter(|entry| entry
                .as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|suffix| suffix == "py"))
            .count(),
        0
    );
    let metadata = metadata();
    let package = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["name"] == "numera-emoji-runtime")
        .unwrap();
    let bins = package["targets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|target| {
            target["kind"]
                .as_array()
                .unwrap()
                .iter()
                .any(|kind| kind == "bin")
        })
        .collect::<Vec<_>>();
    let names = bins
        .iter()
        .map(|target| target["name"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    for required in ["numera-emoji", "numera-runtime-tests", "numera-dev"] {
        assert!(
            names.contains(required),
            "required actual Rust target is missing: {required}"
        );
    }
    assert_eq!(
        names.len(),
        bins.len(),
        "duplicate Cargo binary target names"
    );
    for target in bins {
        assert!(target["src_path"].as_str().unwrap().ends_with(".rs"));
    }
}
// tests.test_entry_points.EntryPointsImport.test_command_modules_preserve_project_data_paths
#[test]
fn test_command_modules_preserve_project_data_paths() {
    let root = Root::new();
    root.install();
    root.raster("coins/logos/png/red.png", [240, 20, 20, 255]);
    let foreign = Root::new();
    let result = root.run_at(&foreign.0, &["make-emoji-pngs"], &[]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(root.0.join("coins/logos/emoji/red.png").is_file());
    assert!(!foreign.0.join("coins").exists());
    assert!(root.0.join("logs").is_dir());
    struct RootJoin(bool);
    impl<'ast> syn::visit::Visit<'ast> for RootJoin {
        fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
            if node.method=="join"&&node.args.first().is_some_and(|arg|matches!(arg,syn::Expr::Lit(value)if matches!(&value.lit,syn::Lit::Str(text)if text.value()=="state_emoji_bot.json")))&&matches!(&*node.receiver,syn::Expr::Field(field)if matches!(&field.member,syn::Member::Named(name)if name=="root")){self.0=true;}
            syn::visit::visit_expr_method_call(self, node);
        }
    }
    let file =
        syn::parse_file(&std::fs::read_to_string(owned_module("emoji_bot")).unwrap()).unwrap();
    let mut found = RootJoin(false);
    syn::visit::Visit::visit_file(&mut found, &file);
    assert!(
        found.0,
        "actual bot offset must remain under configured project root"
    );
}
// tests.test_entry_points.EntryPointsImport.test_every_entry_point_imports
#[test]
fn test_every_entry_point_imports() {
    let root = Root::new();
    root.install();
    for command in commands() {
        let result = root.run(&[&command, "--help"], &[]);
        if command == "emoji-bot" {
            assert_eq!(result.code, 2);
        } else {
            assert_eq!(result.code, 0, "{command}: {}", result.stderr);
        }
        assert!(!result.stderr.contains("panicked"));
    }
    let file = source("runtime/src/lib.rs");
    assert!(
        file.items
            .iter()
            .filter(|item| matches!(item, syn::Item::Mod(_)))
            .count()
            >= 100
    );
}
