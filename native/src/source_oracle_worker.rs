//! Closed historical source-oracle admission and dispatch; the process owner is shared.
use crate::source_git_inventory::Source;
use crate::source_oracle_process::{check, read, write_new};
use serde_json::{Value, json};
use std::path::Path;

pub async fn check_intent(
    root: &Path,
    executable: &Path,
    source: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    crate::source_intent_oracle::source_identity(source)?;
    crate::source_intent_oracle::fixture_identity(fixture)?;
    check(root, executable, "intent", source, &[], fixture).await
}

pub async fn check_resume(
    root: &Path,
    executable: &Path,
    source: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    crate::source_resume_oracle::source_identity(source)?;
    crate::source_resume_oracle::fixture_identity(fixture)?;
    check(root, executable, "resume", source, &[], fixture).await
}

pub async fn check_bot(
    root: &Path,
    executable: &Path,
    source: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    crate::source_bot_oracle::source_identity(source)?;
    crate::source_bot_oracle::fixture_identity(fixture)?;
    check(root, executable, "bot", source, &[], fixture).await
}

pub async fn check_roster(
    root: &Path,
    executable: &Path,
    source: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    crate::source_roster_oracle::source_identity(source)?;
    crate::source_roster_oracle::fixture_identity(fixture)?;
    check(root, executable, "roster", source, &[], fixture).await
}

pub async fn check_reconcile(
    root: &Path,
    executable: &Path,
    source: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    crate::source_reconcile_oracle::source_identity(source)?;
    crate::source_reconcile_oracle::fixture_identity(fixture)?;
    check(root, executable, "reconcile", source, &[], fixture).await
}

pub async fn check_view(
    root: &Path,
    executable: &Path,
    source: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    crate::source_view_oracle::source_identity(source)?;
    crate::source_view_oracle::fixture_identity(fixture)?;
    check(root, executable, "view", source, &[], fixture).await
}

pub async fn check_similarity(
    root: &Path,
    executable: &Path,
    source: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    crate::source_similarity_oracle::source_identity(source)?;
    crate::source_similarity_oracle::fixture_identity(fixture)?;
    check(root, executable, "similarity", source, &[], fixture).await
}

pub async fn check_state_artifacts(
    root: &Path,
    executable: &Path,
    source: &Source,
    generic: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    crate::source_state_artifacts_oracle::source_identity(source, generic)?;
    crate::source_state_artifacts_oracle::fixture_identity(fixture)?;
    check(
        root,
        executable,
        "state-artifacts",
        source,
        &[generic],
        fixture,
    )
    .await
}

pub async fn check_steps(
    root: &Path,
    executable: &Path,
    source: &Source,
    intent: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    crate::source_steps_oracle::source_identity(source, intent)?;
    crate::source_steps_oracle::fixture_identity(fixture)?;
    check(root, executable, "steps", source, &[intent], fixture).await
}

pub async fn check_ingest(
    root: &Path,
    executable: &Path,
    source: &Source,
    near: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    crate::source_ingest_oracle::source_identity(source, near)?;
    crate::source_ingest_oracle::fixture_identity(fixture)?;
    check(root, executable, "ingest", source, &[near], fixture).await
}

pub async fn check_gallery(
    root: &Path,
    executable: &Path,
    source: &Source,
    script: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    crate::source_gallery_oracle::source_identity(source, script)?;
    crate::source_gallery_oracle::fixture_identity(fixture)?;
    check(root, executable, "gallery", source, &[script], fixture).await
}

pub async fn check_signature(
    root: &Path,
    executable: &Path,
    source: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    crate::source_sqlite_oracle::source_identity(source)?;
    crate::source_sqlite_oracle::fixture_signature_identity(fixture)?;
    check(root, executable, "signature", source, &[], fixture).await
}

pub async fn check_database(
    root: &Path,
    executable: &Path,
    source: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    crate::source_sqlite_oracle::source_identity(source)?;
    crate::source_sqlite_oracle::fixture_database_identity(fixture)?;
    check(root, executable, "database", source, &[], fixture).await
}

pub async fn check_roster_pack(
    root: &Path,
    executable: &Path,
    source: &Source,
    rows: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    crate::source_roster_pack_oracle::source_identity(source, rows)?;
    crate::source_roster_pack_oracle::fixture_identity(fixture)?;
    check(root, executable, "roster-pack", source, &[rows], fixture).await
}

pub async fn check_archive(
    root: &Path,
    executable: &Path,
    source: &Source,
    rows: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    crate::source_archive_oracle::source_identity(source, rows)?;
    crate::source_archive_oracle::fixture_identity(fixture)?;
    check(root, executable, "archive", source, &[rows], fixture).await
}

pub async fn check_lottie(
    root: &Path,
    executable: &Path,
    source: &Source,
    errors: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    crate::source_lottie_oracle::source_identity(source, errors)?;
    crate::source_lottie_oracle::fixture_identity(fixture)?;
    check(
        root,
        executable,
        "lottie-validation",
        source,
        &[errors],
        fixture,
    )
    .await
}

pub async fn check_lottie_files(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    source: &Source,
    errors: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    crate::source_lottie_oracle::source_identity(source, errors)?;
    crate::source_lottie_oracle::file_fixture_identity(fixture)?;
    check(
        root,
        executable,
        "lottie-files",
        source,
        &[errors],
        &json!({"workspace":workspace,"fixture":fixture}),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn generate_lottie_render(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    source: &Source,
    errors: &Source,
    preview: &Value,
    gallery: &Value,
) -> Result<Value, String> {
    crate::source_lottie_oracle::source_identity(source, errors)?;
    crate::source_lottie_render::fixture_identity(preview, gallery)?;
    check(
        root,
        executable,
        "lottie-render",
        source,
        &[errors],
        &json!({"workspace":workspace,"preview":preview,"gallery":gallery}),
    )
    .await
}

pub async fn generate_static(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    sources: (&Source, &Source),
    fixtures: (&Value, &Value),
) -> Result<Value, String> {
    let (media, provider) = sources;
    let (static_fixture, provider_fixture) = fixtures;
    crate::source_static_oracle::source_identity(media, provider)?;
    crate::source_static_oracle::fixture_identity(static_fixture, provider_fixture)?;
    check(
        root,
        executable,
        "static-images",
        media,
        &[provider],
        &json!({"workspace":workspace,"static":static_fixture,"provider":provider_fixture}),
    )
    .await
}

pub async fn observe_brand(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    sources: &[&Source],
) -> Result<Value, String> {
    crate::source_brand_oracle::source_identity(sources)?;
    check(
        root,
        executable,
        "brand-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}

pub async fn observe_emoji_ids(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    source: &Source,
    test: &Source,
) -> Result<Value, String> {
    crate::source_emoji_ids_oracle::source_identity(source, test)?;
    check(
        root,
        executable,
        "emoji-id-tests",
        source,
        &[test],
        &json!({"workspace":workspace}),
    )
    .await
}

pub async fn observe_fitting(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    media: &Source,
    test: &Source,
) -> Result<Value, String> {
    crate::source_fitting_oracle::source_identity(media, test)?;
    check(
        root,
        executable,
        "fitting-tests",
        media,
        &[test],
        &json!({"workspace":workspace}),
    )
    .await
}

pub async fn check_coin_audit(
    root: &Path,
    executable: &Path,
    source: &Source,
    environment: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    crate::source_coin_audit_oracle::source_identity(source, environment)?;
    crate::source_coin_audit_oracle::fixture_identity(fixture)?;
    check(
        root,
        executable,
        "coin-audit",
        source,
        &[environment],
        fixture,
    )
    .await
}

pub async fn check_coin_audit_images(
    root: &Path,
    executable: &Path,
    source: &Source,
    media: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    crate::source_coin_audit_oracle::image_source_identity(source, media)?;
    crate::source_coin_audit_oracle::image_fixture_identity(fixture)?;
    check(
        root,
        executable,
        "coin-audit-images",
        source,
        &[media],
        fixture,
    )
    .await
}

pub async fn observe_coin_audit(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    sources: &[&Source],
) -> Result<Value, String> {
    crate::source_coin_audit_tests::source_identity(sources)?;
    check(
        root,
        executable,
        "coin-audit-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}

pub async fn generate_preflight(
    root: &Path,
    executable: &Path,
    source: &Source,
    test: &Source,
) -> Result<Value, String> {
    crate::source_preflight_oracle::source_identity(source, test)?;
    check(root, executable, "preflight", source, &[test], &Value::Null).await
}

pub async fn observe_local_ingest(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    sources: &[&Source],
) -> Result<Value, String> {
    crate::source_local_ingest_oracle::source_identity(sources)?;
    check(
        root,
        executable,
        "local-ingest",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}

pub async fn generate_configuration(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    source: &Source,
    test: &Source,
    numeric: &Source,
    announce: &Source,
) -> Result<Value, String> {
    crate::source_config_oracle::source_identity(source, test, numeric, announce)?;
    check(
        root,
        executable,
        "configuration",
        source,
        &[test, numeric, announce],
        &json!({"workspace":workspace}),
    )
    .await
}

pub async fn generate_logging(
    root: &Path,
    executable: &Path,
    source: &Source,
    test: &Source,
) -> Result<Value, String> {
    crate::source_logging_oracle::source_identity(source, test)?;
    check(root, executable, "logging", source, &[test], &Value::Null).await
}

pub async fn generate_logging_secrets(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    source: &Source,
    test: &Source,
) -> Result<Value, String> {
    crate::source_logging_oracle::source_identity(source, test)?;
    check(
        root,
        executable,
        "logging-secrets",
        source,
        &[test],
        &json!({"workspace":workspace}),
    )
    .await
}

pub async fn generate_logging_retention(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    sources: &[&Source],
) -> Result<Value, String> {
    crate::source_logging_oracle::retention_identity(sources)?;
    check(
        root,
        executable,
        "logging-retention",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}

pub async fn generate_rows(
    root: &Path,
    executable: &Path,
    source: &Source,
    test: &Source,
) -> Result<Value, String> {
    crate::source_rows_oracle::source_identity(source, test)?;
    check(root, executable, "rows", source, &[test], &Value::Null).await
}

pub async fn generate_benchmark_policy(
    root: &Path,
    executable: &Path,
    source: &Source,
    reference: &Source,
) -> Result<Value, String> {
    crate::source_benchmark_oracle::source_identity(source, reference)?;
    check(
        root,
        executable,
        "benchmark-policy",
        source,
        &[reference],
        &Value::Null,
    )
    .await
}

pub async fn benchmark_similarity(
    root: &Path,
    executable: &Path,
    source: &Source,
    catalog: Option<&Path>,
) -> Result<crate::tooling_similarity_benchmark::Report, crate::tooling_similarity_benchmark::Failure>
{
    use crate::tooling_similarity_benchmark::{Failure, Report};
    crate::source_similarity_oracle::source_identity(source).map_err(Failure::Uncaught)?;
    let value = check(
        root,
        executable,
        "benchmark",
        source,
        &[],
        &json!({"catalog":catalog}),
    )
    .await
    .map_err(Failure::Uncaught)?;
    let result: Result<Report, Failure> = serde_json::from_value(value)
        .map_err(|_| Failure::Uncaught("invalid benchmark worker result".into()))?;
    let report = result?;
    report.validate().map_err(Failure::Uncaught)?;
    Ok(report)
}

pub async fn observe_catalog(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    source: &Source,
    paths: &Source,
    test: &Source,
) -> Result<Value, String> {
    crate::source_catalog_oracle::source_identity(source, paths, test)?;
    check(
        root,
        executable,
        "catalog",
        source,
        &[paths, test],
        &json!({"workspace":workspace}),
    )
    .await
}

pub async fn observe_catalog_tests(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    sources: &[&Source],
) -> Result<Value, String> {
    crate::source_catalog_tests_oracle::source_identity(sources)?;
    check(
        root,
        executable,
        "catalog-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}

pub async fn observe_state_compatibility(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    sources: &[&Source],
) -> Result<Value, String> {
    crate::source_state_compat_oracle::source_identity(sources)?;
    check(
        root,
        executable,
        "state-compatibility",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}

pub async fn observe_prebuilt_tests(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    sources: &[&Source],
) -> Result<Value, String> {
    crate::source_prebuilt_oracle::source_identity(sources)?;
    check(
        root,
        executable,
        "prebuilt-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}

pub fn run(root: &Path, args: &[String]) -> u8 {
    let log = std::sync::Arc::new(crate::logging::RunLog::open(
        root,
        "source-oracle-worker",
        vec![],
    ));
    log.install_panic_hook();
    let result = (|| {
        if args.len() != 2 {
            return Err("source-oracle-worker requires REQUEST RESPONSE".into());
        }
        let root = crate::tooling_environment::physical_root(root)?;
        let input = Path::new(&args[0]);
        let output = Path::new(&args[1]);
        if !output.is_absolute() || output == input || std::fs::symlink_metadata(output).is_ok() {
            return Err("source oracle response must be new and distinct".into());
        }
        crate::codec_worker::physical_directory(
            &root,
            output
                .parent()
                .ok_or("source oracle response has no parent")?,
        )?;
        let request = read(&root, input)?;
        let field = |name: &str| {
            request[name]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("source oracle missing {name}"))
        };
        let source = Source {
            baseline_revision: field("baseline_revision")?,
            path: field("path")?,
            sha256: field("sha256")?,
            text: field("source")?,
        };
        let observed = crate::source_oracle_dispatch::observe(&root, &request, &source);
        let (code, response) = match observed {
            Ok(result) => (0, json!({"ok":true,"result":result})),
            Err(error) => (
                1,
                json!({"ok":false,"error":crate::logging::redact(&error,&[])}),
            ),
        };
        write_new(output, &response)?;
        Ok::<u8, String>(code)
    })();
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            log.event("ERROR", "source-oracle", &error);
            2
        }
    };
    log.finish(i32::from(code));
    code
}
