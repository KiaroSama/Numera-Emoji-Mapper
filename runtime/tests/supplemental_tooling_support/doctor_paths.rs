//! Real CLI diagnostics agree with catalog operations and preserve physical path boundaries.
use super::{Root, invoke};
use std::path::{Path, PathBuf};

fn executable(root: &Path) -> PathBuf {
    std::fs::create_dir(root.join("assets")).unwrap();
    std::fs::write(root.join("assets/panel.html"), b"fixture").unwrap();
    let executable = root.join(format!("numera-emoji{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    executable
}

fn directory_link(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    junction::create(target, link).unwrap();
}

fn remove_directory_link(link: &Path) {
    #[cfg(unix)]
    std::fs::remove_file(link).unwrap();
    #[cfg(windows)]
    junction::delete(link).unwrap();
}

fn locations(text: &str, catalog: &Path, workspace: Option<&Path>) {
    assert!(
        text.contains(&format!("Catalog path: {}", catalog.display())),
        "{text}"
    );
    let expected = workspace.map_or_else(
        || "not required (data directory is inside project root)".into(),
        |path| path.display().to_string(),
    );
    assert!(
        text.contains(&format!("Private codec workspace: {expected}")),
        "{text}"
    );
    assert!(!text.contains("unresolved ("), "{text}");
}

#[test]
fn doctor_explicit_selection_matches_real_default_relative_and_external_catalogs() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/native/static-compare/red.png");
    let original = std::fs::read(&source).unwrap();
    for mode in ["default", "relative", "external"] {
        let root = Root::new(&format!("doctor agreement {mode}"));
        let outside = Root::new(&format!("doctor external {mode}"));
        let executable = executable(&root.0);
        let data = match mode {
            "default" => root.0.join("collection"),
            "relative" => root.0.join("selected café/catalog"),
            _ => outside.0.clone(),
        };
        let selection = if mode == "relative" {
            "selected café/catalog"
        } else {
            data.to_str().unwrap()
        };
        let mut doctor = vec!["doctor"];
        let mut import = vec!["add-media", source.to_str().unwrap(), "--as", "static"];
        if mode != "default" {
            doctor.extend(["--data-dir", selection]);
            import.extend(["--data-dir", selection]);
        }
        // This unsupported ambient variable must not select a different catalog for doctor.
        let unused = outside.0.join("unused environment selection");
        let environment = [("EMOJI_DATA_DIR", unused.to_str().unwrap())];
        let checked = invoke(&root.0, &executable, &doctor, &environment);
        assert_eq!(checked.code, 0, "{}", checked.stderr);
        let workspace = root.0.join("logs/tmp/external-data");
        locations(
            &checked.stdout,
            &data.join("catalog.db"),
            (mode == "external").then_some(workspace.as_path()),
        );
        assert!(!data.join("catalog.db").exists());
        assert!(!workspace.exists());
        let imported = invoke(&root.0, &executable, &import, &environment);
        assert_eq!(imported.code, 0, "{}{}", imported.stdout, imported.stderr);
        let db = rusqlite::Connection::open_with_flags(
            data.join("catalog.db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM items", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        db.close().unwrap();
        assert!(!unused.exists());
        assert_eq!(std::fs::read(&source).unwrap(), original);
        outside.clean().unwrap();
        root.clean().unwrap();
    }
}

#[test]
fn doctor_rejects_invalid_selection_before_loading_configuration() {
    let root = Root::new("doctor invalid selectors");
    let executable = executable(&root.0);
    let invalid = b"GENERAL_BOT_TOKEN=\xff";
    std::fs::write(root.0.join(".env"), invalid).unwrap();
    let environment = [("NUMERA_EMOJI_MAPPER_NO_DOTENV", "0")];
    for arguments in [
        vec!["doctor", "--data-dir"],
        vec!["doctor", "--data-dir", ""],
        vec!["doctor", "--data-dir", " \u{2002}\t"],
        vec!["doctor", "--data-dir", "--data-dir"],
        vec!["doctor", "--data-dir", "--unknown"],
        vec!["doctor", "--data-dir", "one", "--data-dir", "two"],
        vec!["doctor", "--unknown"],
        vec!["doctor", "unselected"],
    ] {
        let actual = invoke(&root.0, &executable, &arguments, &environment);
        assert_eq!(actual.code, 1, "{}{}", actual.stdout, actual.stderr);
        assert!(actual.stderr.contains("doctor"), "{}", actual.stderr);
        assert!(!actual.stderr.contains("cannot read configuration"));
        assert!(!actual.stdout.contains("Catalog path:"));
    }
    for help in ["--help", "-h"] {
        let actual = invoke(&root.0, &executable, &["doctor", help], &environment);
        assert_eq!(actual.code, 0, "{}", actual.stderr);
        assert!(actual.stdout.contains("--data-dir DIR"));
    }
    assert_eq!(std::fs::read(root.0.join(".env")).unwrap(), invalid);
    for absent in ["collection", "one", "two", "unselected"] {
        assert!(!root.0.join(absent).exists());
    }
    root.clean().unwrap();
}

#[test]
fn doctor_resolves_missing_parent_components_without_creating_catalogs() {
    let root = Root::new("doctor planned physical paths");
    let outside = Root::new("doctor planned outside");
    let executable = executable(&root.0);
    std::fs::create_dir(root.0.join("existing")).unwrap();
    let outside_name = outside.0.file_name().unwrap();
    let cases = [
        (
            root.0.join("missing/../catalog"),
            root.0.join("catalog"),
            false,
        ),
        (
            root.0.join("missing/nested/../../catalog"),
            root.0.join("catalog"),
            false,
        ),
        (
            root.0.join("existing/../catalog"),
            root.0.join("catalog"),
            false,
        ),
        (
            outside.0.join("missing/../catalog"),
            outside.0.join("catalog"),
            true,
        ),
        (
            root.0
                .join("missing/../..")
                .join(outside_name)
                .join("catalog"),
            outside.0.join("catalog"),
            true,
        ),
        (
            root.0.join("fresh/nested/café catalog/."),
            root.0.join("fresh/nested/café catalog"),
            false,
        ),
    ];
    for (request, expected, external) in cases {
        let actual = invoke(
            &root.0,
            &executable,
            &["doctor", "--data-dir", request.to_str().unwrap()],
            &[],
        );
        assert_eq!(actual.code, 0, "{}", actual.stderr);
        let workspace = root.0.join("logs/tmp/external-data");
        locations(
            &actual.stdout,
            &expected.join("catalog.db"),
            external.then_some(workspace.as_path()),
        );
        assert!(!expected.exists());
        assert!(!workspace.exists());
        assert!(!root.0.join("missing").exists());
        assert!(!root.0.join("fresh").exists());
        assert!(!outside.0.join("missing").exists());
    }
    outside.clean().unwrap();
    root.clean().unwrap();
}

#[test]
fn doctor_refuses_linked_or_file_directories_without_repairing_them() {
    let root = Root::new("doctor invalid physical storage");
    let outside = Root::new("doctor retained outside");
    let executable = executable(&root.0);
    let sentinel = b"retained outside bytes";
    std::fs::write(outside.0.join("sentinel"), sentinel).unwrap();
    let file = root.0.join("file");
    std::fs::write(&file, sentinel).unwrap();
    let link = root.0.join("linked");
    directory_link(&outside.0, &link);
    for request in [
        file.clone(),
        file.join("child"),
        file.join("../catalog"),
        link.clone(),
        link.join("."),
        link.join("../catalog"),
        root.0.join("missing/../linked/catalog"),
        root.0.join("missing/../linked/../catalog"),
    ] {
        let actual = invoke(
            &root.0,
            &executable,
            &["doctor", "--data-dir", request.to_str().unwrap()],
            &[],
        );
        assert_eq!(actual.code, 0, "{}", actual.stderr);
        assert!(actual.stdout.contains("Catalog path: unresolved ("));
        assert!(
            actual
                .stdout
                .contains("Private codec workspace: unresolved (")
        );
        assert!(!root.0.join("missing").exists());
        assert!(!root.0.join("catalog").exists());
        assert!(!outside.0.join("catalog").exists());
        assert_eq!(std::fs::read(&file).unwrap(), sentinel);
        assert_eq!(std::fs::read(outside.0.join("sentinel")).unwrap(), sentinel);
    }
    remove_directory_link(&link);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.0.join("absent"), &link).unwrap();
        let actual = invoke(
            &root.0,
            &executable,
            &["doctor", "--data-dir", link.to_str().unwrap()],
            &[],
        );
        assert!(actual.stdout.contains("Catalog path: unresolved ("));
        assert!(!root.0.join("absent").exists());
        remove_directory_link(&link);
    }
    let workspace = root.0.join("logs/tmp/external-data");
    std::fs::write(&workspace, sentinel).unwrap();
    for linked in [false, true] {
        if linked {
            std::fs::remove_file(&workspace).unwrap();
            directory_link(&outside.0, &workspace);
        }
        let actual = invoke(
            &root.0,
            &executable,
            &["doctor", "--data-dir", outside.0.to_str().unwrap()],
            &[],
        );
        assert_eq!(actual.code, 0, "{}", actual.stderr);
        assert!(actual.stdout.contains(&format!(
            "Catalog path: {}",
            outside.0.join("catalog.db").display()
        )));
        assert!(
            actual
                .stdout
                .contains("Private codec workspace: unresolved (")
        );
        if !linked {
            assert_eq!(std::fs::read(&workspace).unwrap(), sentinel);
        }
        assert_eq!(std::fs::read(outside.0.join("sentinel")).unwrap(), sentinel);
    }
    remove_directory_link(&workspace);
    std::fs::create_dir(&workspace).unwrap();
    let actual = invoke(
        &root.0,
        &executable,
        &["doctor", "--data-dir", outside.0.to_str().unwrap()],
        &[],
    );
    assert_eq!(actual.code, 0, "{}", actual.stderr);
    locations(
        &actual.stdout,
        &outside.0.join("catalog.db"),
        Some(&workspace),
    );
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    assert!(!outside.0.join("catalog.db").exists());
    outside.clean().unwrap();
    root.clean().unwrap();
}

#[cfg(windows)]
#[test]
fn doctor_windows_root_relative_paths_resolve_and_drive_relative_paths_are_explicit() {
    let root = Root::new("doctor Windows path prefixes");
    let executable = executable(&root.0);
    let Some(std::path::Component::Prefix(prefix)) = root.0.components().next() else {
        panic!("Windows fixture has no filesystem prefix");
    };
    let expected = root.0.join("planned catalog");
    let rooted = expected.strip_prefix(prefix.as_os_str()).unwrap();
    let actual = invoke(
        &root.0,
        &executable,
        &["doctor", "--data-dir", rooted.to_str().unwrap()],
        &[],
    );
    assert_eq!(actual.code, 0, "{}", actual.stderr);
    locations(&actual.stdout, &expected.join("catalog.db"), None);
    if matches!(
        prefix.kind(),
        std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
    ) {
        let relative = format!("{}planned catalog", prefix.as_os_str().to_string_lossy());
        let actual = invoke(
            &root.0,
            &executable,
            &["doctor", "--data-dir", &relative],
            &[],
        );
        assert_eq!(actual.code, 0, "{}", actual.stderr);
        assert!(actual.stdout.contains(
            "unresolved (Diagnostic storage requires an absolute or installation-relative path)"
        ));
    }
    assert!(!expected.exists());
    root.clean().unwrap();
}

#[test]
fn doctor_redacts_effective_environment_and_loaded_dotenv_secrets_in_paths() {
    let root = Root::new("doctor effective secret paths");
    let outside = Root::new("doctor secret outside");
    let executable = executable(&root.0);
    let secret = "synthetic-never-issued-doctor-credential";
    let data = outside.0.join(secret);
    for raw in [
        secret.to_owned(),
        format!("  {secret}  "),
        format!("\u{2002}{secret}\u{2003}"),
    ] {
        let actual = invoke(
            &root.0,
            &executable,
            &["doctor", "--data-dir", data.to_str().unwrap()],
            &[("GENERAL_BOT_TOKEN", &raw)],
        );
        assert_eq!(actual.code, 0, "{}", actual.stderr);
        assert!(actual.stdout.contains("[REDACTED]"));
        assert!(!actual.stdout.contains(secret));
        assert!(!actual.stderr.contains(secret));
    }
    for value in [
        format!("  {secret}  "),
        format!("\"  {secret}  \""),
        format!("'\u{2002}{secret}\u{2003}'"),
    ] {
        let dotenv = format!("GENERAL_BOT_TOKEN={value}\n");
        std::fs::write(root.0.join(".env"), &dotenv).unwrap();
        let actual = invoke(
            &root.0,
            &executable,
            &["doctor", "--data-dir", data.to_str().unwrap()],
            &[("NUMERA_EMOJI_MAPPER_NO_DOTENV", "0")],
        );
        assert_eq!(actual.code, 0, "{}", actual.stderr);
        assert!(actual.stdout.contains("[REDACTED]"));
        assert!(!actual.stdout.contains(secret));
        assert!(!actual.stderr.contains(secret));
        assert_eq!(
            std::fs::read_to_string(root.0.join(".env")).unwrap(),
            dotenv
        );
    }
    for entry in std::fs::read_dir(root.0.join("logs")).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            assert!(!std::fs::read_to_string(path).unwrap().contains(secret));
        }
    }
    assert!(!data.exists());
    assert!(!root.0.join("logs/tmp/external-data").exists());
    outside.clean().unwrap();
    root.clean().unwrap();
}
