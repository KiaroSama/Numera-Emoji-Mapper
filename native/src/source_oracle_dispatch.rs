//! Closed historical-oracle selection; every implementation owns its fixed source admission.
use crate::source_git_inventory::Source;
use serde_json::Value;
use std::path::Path;

fn dependency_at(request: &Value, index: usize, count: usize) -> Result<Source, String> {
    let values = request["dependencies"]
        .as_array()
        .filter(|values| values.len() == count)
        .ok_or("historical oracle requires its exact pinned dependencies")?;
    let value = values
        .get(index)
        .ok_or("historical oracle dependency is missing")?;
    let field = |name: &str| {
        value[name]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("oracle dependency missing {name}"))
    };
    Ok(Source {
        baseline_revision: field("baseline_revision")?,
        path: field("path")?,
        sha256: field("sha256")?,
        text: field("source")?,
    })
}

fn dependency(request: &Value) -> Result<Source, String> {
    dependency_at(request, 0, 1)
}

pub(crate) fn observe(root: &Path, request: &Value, source: &Source) -> Result<Value, String> {
    pyo3::Python::initialize();
    pyo3::Python::attach(|py| {
        crate::source_oracle_runtime::host_import(py, "builtins").map(|_| ())
    })
    .map_err(|error| error.to_string())?;
    match request["oracle"].as_str() {
        Some("runtime-discovery") => {
            if request["dependencies"]
                .as_array()
                .is_none_or(|values| !values.is_empty())
            {
                return Err("runtime discovery requires only its pinned original guard".into());
            }
            let stage = request["fixture"]["stage"]
                .as_str()
                .ok_or("runtime discovery requires its admitted stage")?;
            crate::source_runtime_discovery::observe(root, Path::new(stage), source)
        }
        Some("intent") => {
            crate::source_intent_oracle::source_identity(source)?;
            crate::source_intent_oracle::fixture_identity(&request["fixture"])?;
            crate::source_oracle_runtime::without_applications(|| {
                crate::source_intent_oracle::check(source, &request["fixture"])
            })
        }
        Some("resume") => {
            crate::source_resume_oracle::source_identity(source)?;
            crate::source_resume_oracle::fixture_identity(&request["fixture"])?;
            crate::source_oracle_runtime::without_applications(|| {
                crate::source_resume_oracle::check(source, &request["fixture"])
            })
        }
        Some("bot") => crate::source_bot_oracle::check(source, &request["fixture"]),
        Some("bot-tests") => crate::source_bot_tests::observe(source, &dependency(request)?),
        Some("bot-command-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("bot command replay requires its workspace")?;
            let dependencies = (0..3)
                .map(|index| dependency_at(request, index, 3))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_bot_command_tests::observe(root, Path::new(workspace), &sources)
        }
        Some("roster") => crate::source_roster_oracle::check(source, &request["fixture"]),
        Some("reconcile") => crate::source_reconcile_oracle::check(source, &request["fixture"]),
        Some("reconcile-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("reconciliation replay requires its workspace")?;
            let dependencies = (0..33)
                .map(|index| dependency_at(request, index, 33))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_reconcile_tests::observe(root, Path::new(workspace), &sources)
        }
        Some("publication-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("publication replay requires its workspace")?;
            let dependencies = (0..36)
                .map(|index| dependency_at(request, index, 36))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_publication_tests::observe(root, Path::new(workspace), &sources)
        }
        Some("panel-view-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("panel view replay requires its workspace")?;
            let dependencies = (0..35)
                .map(|index| dependency_at(request, index, 35))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_panel_view_tests::observe(root, Path::new(workspace), &sources)
        }
        Some("identity-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("identity replay requires its workspace")?;
            let dependencies = (0..20)
                .map(|index| dependency_at(request, index, 20))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_identity_tests::observe(root, Path::new(workspace), &sources)
        }
        Some("identity-bundle") => {
            let dependencies = (0..20)
                .map(|index| dependency_at(request, index, 20))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            let fixture = &request["fixture"];
            let data = fixture["data"]
                .as_str()
                .ok_or("bundle replay dataset missing")?;
            let manifest = fixture["manifest"].as_str().map(Path::new);
            let action = fixture["action"]
                .as_str()
                .ok_or("bundle replay action missing")?;
            crate::source_identity_tests::observe_bundle(
                root,
                Path::new(data),
                manifest,
                action,
                &sources,
            )
        }
        Some("snapshot-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("snapshot replay requires its workspace")?;
            let dependencies = (0..3)
                .map(|index| dependency_at(request, index, 3))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_snapshot_tests::observe(root, Path::new(workspace), &sources)
        }
        Some("view") => {
            crate::source_view_oracle::source_identity(source)?;
            crate::source_view_oracle::fixture_identity(&request["fixture"])?;
            crate::source_oracle_runtime::without_applications(|| {
                crate::source_view_oracle::check(source, &request["fixture"])
            })
        }
        Some("similarity") => crate::source_similarity_oracle::check(source, &request["fixture"]),
        Some("static-images") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("static image replay requires its workspace")?;
            crate::source_static_oracle::generate(
                root,
                Path::new(workspace),
                source,
                &dependency(request)?,
                &request["fixture"]["static"],
                &request["fixture"]["provider"],
            )
        }
        Some("keyword-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("keyword replay requires its workspace")?;
            crate::source_keywords_tests::observe(
                root,
                Path::new(workspace),
                &[source, &dependency(request)?],
            )
        }
        Some("brand-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("brand replay requires its workspace")?;
            let dependencies = (0..4)
                .map(|index| dependency_at(request, index, 4))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_brand_oracle::observe(root, Path::new(workspace), &sources)
        }
        Some("emoji-id-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("inventory replay requires its workspace")?;
            crate::source_emoji_ids_oracle::observe(
                root,
                Path::new(workspace),
                source,
                &dependency(request)?,
            )
        }
        Some("reencode-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("reencoding assertions require their workspace")?;
            let dependencies = (0..14)
                .map(|index| dependency_at(request, index, 14))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_reencode_tests::observe(root, Path::new(workspace), &sources)
        }
        Some("fitting-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("fitting replay requires its workspace")?;
            crate::source_fitting_oracle::observe(
                root,
                Path::new(workspace),
                source,
                &dependency(request)?,
            )
        }
        Some("lottie-render") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("Lottie render requires its workspace")?;
            crate::source_lottie_render::generate(
                root,
                Path::new(workspace),
                source,
                &dependency(request)?,
                &request["fixture"]["preview"],
                &request["fixture"]["gallery"],
            )
        }
        Some("lottie-files") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("Lottie file replay requires its workspace")?;
            crate::source_lottie_oracle::check_files(
                root,
                Path::new(workspace),
                source,
                &dependency(request)?,
                &request["fixture"]["fixture"],
            )
        }
        Some("lottie-validation") => {
            crate::source_lottie_oracle::check(source, &dependency(request)?, &request["fixture"])
        }
        Some("state-artifacts") => crate::source_state_artifacts_oracle::check(
            source,
            &dependency(request)?,
            &request["fixture"],
        ),
        Some("steps") => {
            let intent = dependency(request)?;
            crate::source_steps_oracle::source_identity(source, &intent)?;
            crate::source_steps_oracle::fixture_identity(&request["fixture"])?;
            crate::source_oracle_runtime::without_applications(|| {
                crate::source_steps_oracle::check(source, &intent, &request["fixture"])
            })
        }
        Some("ingest") => {
            crate::source_ingest_oracle::check(source, &dependency(request)?, &request["fixture"])
        }
        Some("gallery") => crate::source_gallery_oracle::check(
            root,
            source,
            &dependency(request)?,
            &request["fixture"],
        ),
        Some("signature") => {
            crate::source_sqlite_oracle::check_signature(source, &request["fixture"])
        }
        Some("database") => {
            crate::source_sqlite_oracle::check_database(source, &request["fixture"])
        }
        Some("roster-pack") => crate::source_roster_pack_oracle::check(
            source,
            &dependency(request)?,
            &request["fixture"],
        ),
        Some("archive") => {
            crate::source_archive_oracle::check(source, &dependency(request)?, &request["fixture"])
        }
        Some("roster-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("roster tests require their workspace")?;
            let dependencies = (0..17)
                .map(|index| dependency_at(request, index, 17))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_roster_tests::observe(root, Path::new(workspace), &sources)
        }
        Some("rebuild-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("rebuild tests require their workspace")?;
            let dependencies = (0..15)
                .map(|index| dependency_at(request, index, 15))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_rebuild_tests::observe(root, Path::new(workspace), &sources)
        }
        Some("status-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("status tests require their workspace")?;
            let dependencies = (0..8)
                .map(|index| dependency_at(request, index, 8))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_status_tests::observe(root, Path::new(workspace), &sources)
        }
        Some("export-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("export tests require their workspace")?;
            let dependencies = (0..8)
                .map(|index| dependency_at(request, index, 8))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_export_tests::observe(root, Path::new(workspace), &sources)
        }
        Some("archive-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("archive tests require their workspace")?;
            let dependencies = (0..6)
                .map(|index| dependency_at(request, index, 6))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_archive_tests::observe(root, Path::new(workspace), &sources)
        }
        Some("provider-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("provider assertions require their workspace")?;
            let dependencies = (0..24)
                .map(|index| dependency_at(request, index, 24))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_provider_tests::observe(root, Path::new(workspace), &sources)
        }
        Some("provider-image-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("provider image tests require their workspace")?;
            let dependencies = (0..3)
                .map(|index| dependency_at(request, index, 3))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_provider_image_tests::observe(root, Path::new(workspace), &sources)
        }
        Some("logo-cache-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("logo cache tests require their workspace")?;
            let dependencies = (0..2)
                .map(|index| dependency_at(request, index, 2))
                .collect::<Result<Vec<_>, _>>()?;
            crate::source_logo_cache_tests::observe(
                root,
                Path::new(workspace),
                &[source, &dependencies[0], &dependencies[1]],
            )
        }
        Some("coin-cli-tests") => {
            let dependencies = (0..6)
                .map(|index| dependency_at(request, index, 6))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_coin_cli_tests::observe(root, &sources)
        }
        Some("http-tests") => {
            let dependencies = (0..4)
                .map(|index| dependency_at(request, index, 4))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_http_tests::observe(root, &sources)
        }
        Some("coin-audit-images") => crate::source_coin_audit_oracle::check_images(
            root,
            source,
            &dependency(request)?,
            &request["fixture"],
        ),
        Some("coin-audit") => crate::source_coin_audit_oracle::check(
            source,
            &dependency(request)?,
            &request["fixture"],
        ),
        Some("coin-audit-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("audit tests require their workspace")?;
            let dependencies = (0..5)
                .map(|index| dependency_at(request, index, 5))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_coin_audit_tests::observe(root, Path::new(workspace), &sources)
        }
        Some("order-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("order tests require their workspace")?;
            let dependencies = (0..3)
                .map(|index| dependency_at(request, index, 3))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_sync_order_oracle::observe(root, Path::new(workspace), &sources)
        }
        Some("operator-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("operator tests require their workspace")?;
            let dependencies = (0..2)
                .map(|index| dependency_at(request, index, 2))
                .collect::<Result<Vec<_>, _>>()?;
            crate::source_operator_oracle::observe(
                root,
                Path::new(workspace),
                &[source, &dependencies[0], &dependencies[1]],
            )
        }
        Some("preflight") => {
            crate::source_preflight_oracle::generate(source, &dependency(request)?)
        }
        Some("preflight-outcomes") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("preflight assertions require their workspace")?;
            let dependencies = (0..10)
                .map(|index| dependency_at(request, index, 10))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_preflight_oracle::observe(root, Path::new(workspace), &sources)
        }
        Some("collection-state-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("collection state replay requires its workspace")?;
            let writer = dependency_at(request, 0, 2)?;
            let test = dependency_at(request, 1, 2)?;
            crate::source_collection_state_oracle::observe(
                root,
                Path::new(workspace),
                &[source, &writer, &test],
            )
        }
        Some("state-compatibility") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("state compatibility requires its workspace")?;
            let reference = dependency_at(request, 0, 5)?;
            let test = dependency_at(request, 1, 5)?;
            let diagnostics = dependency_at(request, 2, 5)?;
            let shape = dependency_at(request, 3, 5)?;
            let storage = dependency_at(request, 4, 5)?;
            crate::source_state_compat_oracle::observe(
                root,
                Path::new(workspace),
                &[source, &reference, &test, &diagnostics, &shape, &storage],
            )
        }
        Some("local-ingest") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("local ingest replay requires its workspace")?;
            let dependencies = (0..10)
                .map(|index| dependency_at(request, index, 10))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_local_ingest_oracle::observe(root, Path::new(workspace), &sources)
        }
        Some("tooling-sandbox") => {
            if request["dependencies"]
                .as_array()
                .is_none_or(|values| !values.is_empty())
            {
                return Err("sandbox policy requires no source dependencies".into());
            }
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("sandbox policy requires its workspace")?;
            crate::source_sandbox_oracle::observe(
                root,
                Path::new(workspace),
                source,
                &request["fixture"]["document"],
            )
        }
        Some("tooling-package") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("package replay requires its workspace")?;
            crate::source_package_oracle::observe(
                root,
                Path::new(workspace),
                source,
                &dependency(request)?,
                &request["fixture"]["document"],
            )
        }
        Some("tooling-contracts") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("tooling replay requires its workspace")?;
            crate::source_tooling_oracle::observe(
                root,
                Path::new(workspace),
                source,
                &dependency(request)?,
                &request["fixture"]["document"],
            )
        }
        Some("prebuilt-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("prebuilt test replay requires its workspace")?;
            let dependencies = (0..4)
                .map(|index| dependency_at(request, index, 4))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_prebuilt_oracle::observe(root, Path::new(workspace), &sources)
        }
        Some("catalog-tests") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("catalog test replay requires its workspace")?;
            let dependencies = (0..14)
                .map(|index| dependency_at(request, index, 14))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sources = vec![source];
            sources.extend(dependencies.iter());
            crate::source_catalog_tests_oracle::observe(root, Path::new(workspace), &sources)
        }
        Some("catalog") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("catalog observation requires its workspace")?;
            crate::source_catalog_oracle::observe(
                root,
                Path::new(workspace),
                source,
                &dependency_at(request, 0, 2)?,
                &dependency_at(request, 1, 2)?,
            )
        }
        Some("configuration") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("configuration replay requires its workspace")?;
            crate::source_config_oracle::generate(
                root,
                Path::new(workspace),
                source,
                &dependency_at(request, 0, 3)?,
                &dependency_at(request, 1, 3)?,
                &dependency_at(request, 2, 3)?,
            )
        }
        Some("logging-secrets") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("committed-secret replay requires its workspace")?;
            crate::source_logging_secrets::generate(
                root,
                Path::new(workspace),
                source,
                &dependency(request)?,
            )
        }
        Some("logging-retention") => {
            let workspace = request["fixture"]["workspace"]
                .as_str()
                .ok_or("logging retention requires its workspace")?;
            let test = dependency_at(request, 0, 2)?;
            let environment = dependency_at(request, 1, 2)?;
            crate::source_logging_oracle::generate_retention(
                root,
                Path::new(workspace),
                &[source, &test, &environment],
            )
        }
        Some("logging") => crate::source_logging_oracle::generate(source, &dependency(request)?),
        Some("rows") => crate::source_rows_oracle::generate(source, &dependency(request)?),
        Some("benchmark-policy") => {
            crate::source_benchmark_oracle::generate(source, &dependency(request)?)
        }
        Some("benchmark") => {
            let catalog = match request["fixture"].get("catalog") {
                Some(Value::Null) => None,
                Some(Value::String(path)) if Path::new(path).is_absolute() => Some(Path::new(path)),
                _ => return Err("benchmark catalog must be null or an absolute path".into()),
            };
            serde_json::to_value(crate::tooling_similarity_benchmark::run(source, catalog))
                .map_err(|_| "cannot encode benchmark result".into())
        }
        _ => Err("unsupported historical source oracle".into()),
    }
}
