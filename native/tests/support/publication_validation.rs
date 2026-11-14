//! Original dry-run, naming, branding and stable family-lock contracts at the command seam.
use super::{Commands, UTC, me};
use crate::bot_api::Server;
use _native::{catalog::Catalog, locks::Lease};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn call(
    commands: &Commands<'_>,
    data: &Path,
    environment: &BTreeMap<String, String>,
    extra: &[&str],
) -> _native::tooling_process::Completed {
    let mut server = Server::new(vec![]);
    let mut environment = environment.clone();
    environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
    let mut argv = vec![
        commands.executable.as_os_str().to_owned(),
        "build-collection".into(),
        "--base".into(),
        "fixture".into(),
        "--title".into(),
        "Fixture".into(),
        "--data-dir".into(),
        data.as_os_str().to_owned(),
        "--dry-run".into(),
    ];
    argv.extend(extra.iter().map(|value| std::ffi::OsString::from(*value)));
    let result = commands.capture(&argv, &environment);
    assert!(server.finish().unwrap().is_empty());
    result
}

pub(super) fn run(commands: &Commands<'_>, images: &[PathBuf; 3]) {
    let data = commands.root.join("publication-validation");
    let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
    let keys = (0..200)
        .map(|index| format!("s:validation{index:030}"))
        .collect::<Vec<_>>();
    for key in &keys {
        assert!(
            cat.insert(
                &json!({"content_key":key,"fmt":"static","file_path":images[0],
            "emojis":["😀"],"keywords":["synthetic"]}),
                UTC
            )
            .unwrap()
        );
    }
    let rows = cat.all(None).unwrap();
    drop(cat);
    let mut environment = commands.environment.clone();
    environment.remove("GENERAL_BOT_TOKEN");
    environment.remove("PACK_OWNER_USER_ID");
    environment.remove("BRAND_LOGO_BOTS");
    environment.remove("BRAND_LOGO_PATH");
    for extra in [
        vec!["--formats", "garbage"],
        vec!["--formats", "static,static"],
        vec!["--formats", "static,"],
        vec!["--formats", ""],
        vec!["--per-set", "0"],
        vec!["--per-set", "-5"],
        vec!["--per-set", "201"],
        vec!["--new-set", "--into-pack", "1"],
        vec!["--into-pack", "0"],
        vec!["--into-pack", "-1"],
    ] {
        let mut options = vec!["--no-brand-logo"];
        options.extend(extra);
        let result = call(commands, &data, &environment, &options);
        assert_eq!(
            result.code, 2,
            "{options:?}: {} {}",
            result.stdout, result.stderr
        );
        assert!(!result.stdout.contains("DONE"));
    }
    for base in ["bad__two", "_lead", "9start", "trail_", "has space", ""] {
        let result = call(
            commands,
            &data,
            &environment,
            &["--no-brand-logo", "--base", base],
        );
        assert_eq!(
            result.code, 2,
            "{base}: {} {}",
            result.stdout, result.stderr
        );
    }
    for base in ["YourBrand_Emoji_Packs", "mypack"] {
        let result = call(
            commands,
            &data,
            &environment,
            &["--no-brand-logo", "--base", base],
        );
        assert_eq!(
            result.code, 0,
            "{base}: {} {}",
            result.stdout, result.stderr
        );
        assert!(result.stdout.contains("DRY RUN: nothing uploaded."));
        assert!(result.stdout.contains(&format!("named {base}s1_by_<bot>")));
        assert!(!data.join(format!("publish_plan_{base}.json")).exists());
        assert!(!data.join(format!("publish_{base}.json")).exists());
    }
    let missing = call(commands, &data, &environment, &[]);
    assert_eq!(missing.code, 2);
    assert!(missing.stderr.contains("BRAND_LOGO_BOTS"));
    let disabled = call(commands, &data, &environment, &["--no-brand-logo"]);
    assert_eq!(disabled.code, 0, "{} {}", disabled.stdout, disabled.stderr);
    environment.insert("BRAND_LOGO_BOTS".into(), "YourEmojiBot".into());
    environment.insert(
        "BRAND_LOGO_PATH".into(),
        data.join("missing.png").to_str().unwrap().into(),
    );
    let missing = call(commands, &data, &environment, &[]);
    assert_eq!(missing.code, 2);
    assert!(missing.stderr.contains("BRAND_LOGO_PATH"));
    environment.insert("BRAND_LOGO_PATH".into(), images[1].to_str().unwrap().into());
    let no_room = call(commands, &data, &environment, &["--per-set", "1"]);
    assert_eq!(no_room.code, 2);
    assert!(no_room.stderr.contains("leaves no room"));
    let branded = call(commands, &data, &environment, &["--formats", "static"]);
    assert_eq!(branded.code, 0, "{} {}", branded.stdout, branded.stderr);
    assert!(
        branded
            .stdout
            .contains("200 emoji -> 2 set(s) of up to 199 (+1 brand logo each)")
    );
    let unbranded = call(
        commands,
        &data,
        &environment,
        &["--formats", "static", "--no-brand-logo"],
    );
    assert_eq!(unbranded.code, 0);
    assert!(
        unbranded
            .stdout
            .contains("200 emoji -> 1 set(s) of up to 200")
    );
    assert!(!unbranded.stdout.contains("brand logo"));
    let lock_path = data.join("publish_fixture.lock");
    {
        let lease = Lease::acquire(&lock_path, UTC).unwrap();
        let blocked = call(commands, &data, &environment, &["--no-brand-logo"]);
        assert_eq!(blocked.code, 4, "{} {}", blocked.stdout, blocked.stderr);
        assert!(!blocked.stdout.contains("DRY RUN"));
        drop(lease);
    }
    assert!(lock_path.is_file());
    for _ in 0..2 {
        let result = call(commands, &data, &environment, &["--no-brand-logo"]);
        assert_eq!(result.code, 0, "{} {}", result.stdout, result.stderr);
    }
    let lease = Lease::acquire(&lock_path, UTC).expect("released means available, not deleted");
    drop(lease);
    assert!(lock_path.is_file());
    let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
    assert_eq!(cat.all(None).unwrap(), rows);
    cat.inclusion(&[keys[0].clone()]).unwrap();
    cat.uploaded(&json!({"content_key":keys[1],"custom_emoji_id":"1","base":"fixture","set_name":"fixture1_by_YourEmojiBot"}), UTC).unwrap();
    drop(cat);
    let filtered = call(
        commands,
        &data,
        &environment,
        &["--no-brand-logo", "--formats", "static"],
    );
    assert_eq!(filtered.code, 0);
    assert!(
        filtered
            .stdout
            .contains("198 emoji -> 1 set(s) of up to 200")
    );
    assert!(!data.join("publish_fixture.json").exists());
    assert!(!data.join("publish_plan_fixture.json").exists());
    assert!(!_native::process_owner::cleanup_pending(commands.root));

    // The real bot suffix is known only after getMe; reject length before plan freeze.
    let mut server = Server::new(vec![me()]);
    let mut environment = commands.environment.clone();
    environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
    let argv = vec![
        commands.executable.as_os_str().to_owned(),
        "build-collection".into(),
        "--base".into(),
        "x".repeat(50).into(),
        "--title".into(),
        "Fixture".into(),
        "--no-brand-logo".into(),
        "--user-id".into(),
        "111111111".into(),
        "--data-dir".into(),
        data.as_os_str().to_owned(),
    ];
    let long = commands.capture(&argv, &environment);
    assert_eq!(long.code, 2);
    assert!(long.stderr.contains("64-character"));
    let requests = server.finish().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "getMe");
    assert!(requests[0].form.is_empty());
    let bad = data.join("corrupt.png");
    std::fs::write(&bad, b"not an image").unwrap();
    let before = Catalog::open(&data.join("catalog.db"), UTC, commands.root)
        .unwrap()
        .all(None)
        .unwrap();
    let mut server = Server::new(vec![me()]);
    let broken = super::execute(commands, &data, &[], &server, Some(&bad));
    assert_eq!(broken.code, 2, "{} {}", broken.stdout, broken.stderr);
    assert!(broken.stderr.contains("cannot be used"));
    assert!(!broken.stdout.contains("DONE"));
    let requests = server.finish().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "getMe");
    assert!(requests[0].form.is_empty());
    assert_eq!(std::fs::read(&bad).unwrap(), b"not an image");
    let after = Catalog::open(&data.join("catalog.db"), UTC, commands.root)
        .unwrap()
        .all(None)
        .unwrap();
    assert_eq!(before, after);
    assert!(!data.join("publish_fixture.json").exists());
    assert!(!data.join("publish_plan_fixture.json").exists());
    assert!(
        !data
            .join(format!("publish_plan_{}.json", "x".repeat(50)))
            .exists()
    );
    assert!(
        !data
            .join(format!("publish_{}.json", "x".repeat(50)))
            .exists()
    );
    let state_path = data.join("publish_fixture.json");
    _native::atomic::write_json(&state_path, &json!({"base":"fixture","sets":[{
        "name":"fixtures1_by_YourEmojiBot","fmt":"static","index":1,"title":"Fixture 1",
        "live":1,"logo":false,"keys":[keys[1]]}],"sent":[],"sent_full":[],"skipped":[],"in_flight":null})).unwrap();
    let original = std::fs::read(&state_path).unwrap();
    let rows = Catalog::open(&data.join("catalog.db"), UTC, commands.root)
        .unwrap()
        .all(None)
        .unwrap();
    let result = call(
        commands,
        &data,
        &environment,
        &["--no-brand-logo", "--mixed"],
    );
    assert_eq!(result.code, 2, "{} {}", result.stdout, result.stderr);
    assert!(
        result
            .stderr
            .contains("cannot change this family's mixed/per-format")
    );
    assert_eq!(std::fs::read(&state_path).unwrap(), original);
    assert!(!data.join("publish_plan_fixture.json").exists());
    assert_eq!(
        Catalog::open(&data.join("catalog.db"), UTC, commands.root)
            .unwrap()
            .all(None)
            .unwrap(),
        rows
    );
}
