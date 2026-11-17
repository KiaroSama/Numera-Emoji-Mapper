use numera_test_inventory::{Manifest, ast, native, refresh, verify, verify_javascript_map};
use std::path::PathBuf;
fn run() -> Result<bool, String> {
    let mut root = PathBuf::from(".");
    let mut manifest = PathBuf::from("tests/fixtures/migration/reference-cases.json");
    let mut native_manifest = PathBuf::from("tests/fixtures/migration/original-main-rust.json");
    let mut output = None;
    let mut playwright = None;
    let mut rust_only = false;
    let mut language_only = false;
    let mut update = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("missing value for{arg}"));
        match arg.as_str() {
            "--root" => root = PathBuf::from(value()?),
            "--manifest" => manifest = PathBuf::from(value()?),
            "--native-manifest" => native_manifest = PathBuf::from(value()?),
            "--output" => output = Some(PathBuf::from(value()?)),
            "--playwright-list" => playwright = Some(PathBuf::from(value()?)),
            "--mode" => match value()?.as_str() {
                "rust" => {
                    rust_only = true;
                    language_only = false;
                }
                "all" => {
                    rust_only = false;
                    language_only = false;
                }
                "language" => language_only = true,
                _ => return Err("mode must be rust, all or language".into()),
            },
            "--refresh" => update = true,
            "--help" => {
                println!(
                    "numera-test-inventory [--root .] [--manifest tests/fixtures/migration/reference-cases.json] [--mode rust|all|language] [--playwright-list FILE] [--output FILE] [--refresh]\nChecks wiring and immutable source receipts; does not prove behavior or test execution."
                );
                return Ok(true);
            }
            _ => return Err(format!("unknown argument{arg}")),
        }
    }
    if language_only {
        if update {
            return Err("language mode cannot refresh original test receipts".into());
        }
        let policy = numera_test_inventory::language::default_policy()?;
        let candidates = numera_test_inventory::language::git_candidates(&root)?;
        let report = numera_test_inventory::language::check(&root, &policy, &candidates);
        if let Some(path) = output {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut bytes = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
            bytes.push(b'\n');
            std::fs::write(path, bytes).map_err(|e| e.to_string())?;
        }
        println!(
            "language policy: {} candidate files; {} violations",
            report.checked_files,
            report.findings.len()
        );
        for finding in report.findings.iter().take(30) {
            eprintln!("{} [{}]: {}", finding.path, finding.rule, finding.message);
        }
        return Ok(report.findings.is_empty());
    }
    let path = root.join(manifest);
    let mut source: Manifest =
        serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let roots = ast::cargo_targets(&root)?;
    let inventory = ast::collect_cargo_targets(&root, &roots);
    let native_path = root.join(native_manifest);
    let mut native_source: native::Manifest =
        serde_json::from_slice(&std::fs::read(&native_path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut refresh_errors = Vec::new();
    if update {
        refresh_errors = numera_test_inventory::validate_source(&source);
        refresh_errors.extend(refresh(&mut source, &inventory));
        if native::source_digest(&native_source) != native::SOURCE_SHA256 {
            refresh_errors.push("original-main source receipts differ; refusing refresh".into());
        }
        native::refresh(&mut native_source, &inventory);
        if refresh_errors.is_empty() {
            let mut bytes = serde_json::to_vec_pretty(&source).map_err(|e| e.to_string())?;
            bytes.push(b'\n');
            std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
            let mut bytes = serde_json::to_vec_pretty(&native_source).map_err(|e| e.to_string())?;
            bytes.push(b'\n');
            std::fs::write(&native_path, bytes).map_err(|e| e.to_string())?;
        }
    }
    let list: Option<serde_json::Value> = playwright
        .map(|p| {
            std::fs::read(p)
                .map_err(|e| e.to_string())
                .and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()))
        })
        .transpose()?;
    let mut native_report = native::verify(&native_source, &inventory);
    native_report
        .errors
        .extend(native::verify_fixtures(&native_source, &root));
    let supplemental = numera_test_inventory::supplemental::check(
        &root,
        &inventory,
        &numera_test_inventory::supplemental::default_retirements()?,
    );
    let mut report = verify(&source, inventory, list.as_ref(), rust_only);
    report.errors.extend(supplemental.errors.clone());
    report.supplemental_wiring = Some(supplemental);
    report.errors.extend(native_report.errors.clone());
    report.original_main_rust = Some(native_report);
    let javascript_map: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("assets/tests/case-map.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    report
        .errors
        .extend(verify_javascript_map(&source, &javascript_map));
    let javascript_root = root
        .join("assets/tests")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    for case in source.cases.iter().filter(|c| c.kind != "backend") {
        if let Some(target) = &case.target {
            let path = root
                .join(&target.file)
                .canonicalize()
                .map_err(|e| e.to_string())?;
            if path.starts_with(&javascript_root) && path.extension().is_some_and(|ext| ext == "js")
            {
                report.javascript_source_sha256.insert(
                    target.file.clone(),
                    ast::digest(&std::fs::read(path).map_err(|e| e.to_string())?),
                );
            } else {
                report.errors.push(format!(
                    "JavaScript target is outside public test sources: {}",
                    target.file
                ));
            }
        }
    }
    report.errors.extend(refresh_errors);
    if let Some(path) = output {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut bytes = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        std::fs::write(path, bytes).map_err(|e| e.to_string())?;
    }
    println!(
        "reference:{}; original IDs:{}; mappings Rust/browser/source:{}/{}/{}; wired Rust functions:{}; JavaScript collection:{}; errors:{}",
        report.source_revision,
        report.manifest_ids,
        report.rust_mappings,
        report.browser_mappings,
        report.served_source_mappings,
        report.rust_discovered_functions,
        report.javascript_collection_checked,
        report.errors.len()
    );
    for error in report.errors.iter().take(30) {
        eprintln!("{error}");
    }
    if report.errors.len() > 30 {
        eprintln!(
            "{} further errors are in the JSON report",
            report.errors.len() - 30
        );
    }
    Ok(report.errors.is_empty())
}
fn main() {
    match run() {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("test inventory: {error}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
#[path = "../../../runtime/tests/support/test_admission.rs"]
mod test_admission;
