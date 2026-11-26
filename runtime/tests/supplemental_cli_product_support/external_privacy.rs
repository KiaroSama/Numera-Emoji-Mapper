use super::*;

#[cfg(unix)]
mod access {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    pub fn share(path: &Path) {
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    pub fn restrict(path: &Path) {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    pub fn snapshot(path: &Path) -> String {
        format!(
            "{:o}:{}",
            fs::metadata(path).unwrap().mode() & 0o777,
            fs::metadata(path).unwrap().uid()
        )
    }
    pub fn verify_private(base: &Path, data: &Path, _: &Path) {
        assert_eq!(snapshot(base), snapshot(data));
        assert_eq!(fs::metadata(base).unwrap().mode() & 0o777, 0o700);
    }
}
#[cfg(windows)]
#[path = "external_privacy_windows.rs"]
mod access;

#[test]
fn private_external_workspace_is_reused_without_touching_retained_material() {
    let app = Installation::new();
    let parent = app.project.join("logs/tmp");
    #[cfg(unix)]
    for path in [&app.project, &app.project.join("logs")] {
        access::share(path);
    }
    access::share(&parent);
    access::restrict(&app.data);
    let config =
        numera_runtime::config::Config::load_with_environment(app.project.clone(), &app.env)
            .unwrap();
    let temporary = config.codec_directory(&app.data, "tmp", true).unwrap();
    let base = parent.join("external-data");
    assert_eq!(temporary, app.base());
    let retained = temporary.join("owned-retained-input");
    fs::write(&retained, b"retained private input").unwrap();
    access::verify_private(&base, &app.data, &retained);
    let before = access::snapshot(&base);
    let source = app.outer.0.join("private artwork.png");
    let bytes = frozen(STATIC);
    fs::write(&source, &bytes).unwrap();
    for counts in ["new=1 dedup=0 failed=0", "new=0 dedup=1 failed=0"] {
        let out = success(app.run(&["add-media", source.to_str().unwrap()], &app.data));
        assert!(out.contains(counts), "{out}");
        assert_eq!(fs::read(&retained).unwrap(), b"retained private input");
        assert_eq!(access::snapshot(&base), before);
        assert_eq!(fs::read_dir(&temporary).unwrap().count(), 1);
    }
    assert_eq!(fs::read(source).unwrap(), bytes);
}
#[test]
fn unsafe_existing_external_workspace_is_refused_without_repair_or_cleanup() {
    let app = Installation::new();
    access::restrict(&app.data);
    let base = app.project.join("logs/tmp/external-data");
    fs::create_dir(&base).unwrap();
    access::share(&base);
    let retained = base.join("retained caller evidence");
    fs::write(&retained, b"do not remove").unwrap();
    let source = app.outer.0.join("private incoming.png");
    let bytes = frozen(STATIC);
    fs::write(&source, &bytes).unwrap();
    let before = access::snapshot(&base);
    let out = app.run(&["add-media", source.to_str().unwrap()], &app.data);
    assert_ne!(out.code, 0);
    assert!(
        out.stderr.contains("private external codec workspace"),
        "{}",
        out.stderr
    );
    assert_eq!(access::snapshot(&base), before);
    assert_eq!(fs::read_dir(&base).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&app.data).unwrap().count(), 0);
    assert_eq!(fs::read(&retained).unwrap(), b"do not remove");
    assert_eq!(fs::read(source).unwrap(), bytes);
}

#[cfg(windows)]
#[test]
fn a_shared_child_dacl_is_refused_even_beneath_a_private_external_workspace() {
    let app = Installation::new();
    let config =
        numera_runtime::config::Config::load_with_environment(app.project.clone(), &app.env)
            .unwrap();
    let tmp = config.codec_directory(&app.data, "tmp", true).unwrap();
    access::share(&tmp);
    let retained = tmp.join("existing input");
    fs::write(&retained, b"retained input").unwrap();
    let before = access::snapshot(&tmp);
    let err = config
        .codec_directory(&app.data, "tmp/native-codec", false)
        .unwrap_err();
    assert!(err.contains("private external codec workspace"), "{err}");
    assert_eq!(access::snapshot(&tmp), before);
    assert_eq!(fs::read(retained).unwrap(), b"retained input");
    assert!(!tmp.join("native-codec").exists());
}

#[cfg(windows)]
#[test]
fn null_empty_and_inexact_external_dacls_are_refused_without_changes() {
    for variant in [
        "null",
        "empty",
        "no-inheritance",
        "read-only",
        "other-trustee",
    ] {
        let app = Installation::new();
        let config =
            numera_runtime::config::Config::load_with_environment(app.project.clone(), &app.env)
                .unwrap();
        let tmp = config.codec_directory(&app.data, "tmp", true).unwrap();
        let base = tmp.parent().unwrap();
        let descriptor = access::snapshot(base);
        let owner = descriptor
            .strip_prefix("O:")
            .unwrap()
            .split_once("D:")
            .unwrap()
            .0;
        let private = format!("D:P(A;OICI;FA;;;{owner})");
        let unsafe_dacl = match variant {
            "null" => None,
            "empty" => Some("D:P".to_owned()),
            "no-inheritance" => Some(format!("D:P(A;;FA;;;{owner})")),
            "read-only" => Some(format!("D:P(A;OICI;FR;;;{owner})")),
            "other-trustee" => Some("D:P(A;OICI;FA;;;WD)".to_owned()),
            _ => unreachable!(),
        };
        let retained = base.join("retained private input");
        fs::write(&retained, b"preserve the owned fixture").unwrap();
        access::permissions(base, unsafe_dacl.as_deref());
        let before = access::snapshot(base);
        let result = config.codec_directory(&app.data, "tmp/native-codec", false);
        let after = access::snapshot(base);
        // Restore only our fixture after recording the production call's unmodified DACL.
        // An empty DACL denies data access, so reads and cleanup follow this restoration.
        access::permissions(base, Some(&private));
        let err = result.unwrap_err();
        assert!(
            err.contains("private external codec workspace"),
            "{variant}: {err}"
        );
        assert_eq!(after, before, "{variant}");
        assert_eq!(
            fs::read(&retained).unwrap(),
            b"preserve the owned fixture",
            "{variant}"
        );
        assert_eq!(fs::read_dir(base).unwrap().count(), 2, "{variant}");
        assert!(!tmp.join("native-codec").exists(), "{variant}");
        assert_eq!(fs::read_dir(&app.data).unwrap().count(), 0, "{variant}");
    }
}

#[test]
fn installed_import_preserves_failed_request_and_response_inside_private_diagnostics() {
    let app = Installation::new();
    let logs = app.project.join("logs");
    access::share(&logs);
    access::restrict(&app.data);
    let source = app.data.join("private original café.png");
    let bytes = b"synthetic undecodable artwork";
    fs::write(&source, bytes).unwrap();
    let result = app.run(&["add-media", source.to_str().unwrap()], &app.data);
    assert_eq!(result.code, 4, "{} {}", result.stdout, result.stderr);
    assert!(
        result.stdout.contains("new=0 dedup=0 failed=1"),
        "{}",
        result.stdout
    );
    let diagnostics: Vec<_> = fs::read_dir(&logs)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("native-codec-diagnostics-")
        })
        .collect();
    assert_eq!(diagnostics.len(), 1);
    let diagnostic = &diagnostics[0];
    let envelope = diagnostic.join("envelope");
    let request = envelope.join("request.json");
    let response = envelope.join("response.json");
    for file in [&request, &response] {
        access::verify_private(diagnostic, &app.data, file);
    }
    let req: Value = serde_json::from_slice(&fs::read(request).unwrap()).unwrap();
    assert_eq!(req["operation"], "convert");
    assert_eq!(
        Path::new(req["source"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        source.canonicalize().unwrap()
    );
    let answer: Value = serde_json::from_slice(&fs::read(response).unwrap()).unwrap();
    assert_eq!(answer["ok"], false);
    assert!(
        answer["error"]
            .as_str()
            .unwrap()
            .contains("cannot identify image")
    );
    assert!(!envelope.join("owner").exists());
    empty(&app.base());
    assert_eq!(fs::read(source).unwrap(), bytes);
}
