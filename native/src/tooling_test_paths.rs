//! Public closure path/role policy shared by plan snapshots, observations and retained capture.
use super::{refusal, text};

pub(crate) fn public_path(name: &str) -> bool {
    // This exact public template is a compiled test input, never the operator's environment.
    if name == ".env.example" {
        return true;
    }
    text(name)
        && !name.contains(['\\', ':'])
        && name.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && (!part.starts_with('.') || matches!(part, ".cargo" | ".github"))
                && !matches!(
                    part.to_ascii_lowercase().as_str(),
                    "private"
                        | "operator"
                        | "secrets.md"
                        | "claude.md"
                        | "agents.md"
                        | "explain-ai.md"
                        | "reference.md"
                        | "credentials"
                        | "credentials.json"
                )
                && ![".pem", ".key", ".pfx", ".p12", ".env"]
                    .iter()
                    .any(|ext| part.to_ascii_lowercase().ends_with(ext))
        })
}
pub(crate) fn member_path(role: &str, name: &str) -> Result<(), String> {
    let allowed = match role {
        "source" => {
            (["tests/", "emojikit/", "coins/", "scripts/"]
                .iter()
                .any(|p| name.starts_with(p))
                && name.ends_with(".py"))
                || matches!(
                    name,
                    "run.ps1" | "coins/run_convert.ps1" | "coins/run_rebuild_loop.ps1"
                )
                || (name.starts_with("scripts/") && name.ends_with(".ps1"))
        }
        "target" => {
            ["native/src/", "native/tests/"]
                .iter()
                .any(|p| name.starts_with(p))
                && name.ends_with(".rs")
        }
        "fixture" => ["tests/fixtures/", "native/tests/fixtures/"]
            .iter()
            .any(|p| name.starts_with(p)),
        "support" => {
            ([
                "native/src/",
                "native/tests/support/",
                "tests/",
                "emojikit/",
                "coins/",
                "scripts/",
                "assets/",
            ]
            .iter()
            .any(|p| name.starts_with(p))
                && [".rs", ".py", ".json", ".html", ".js", ".css", ".toml"]
                    .iter()
                    .any(|p| name.ends_with(p)))
                || (name.starts_with("assets/")
                    && name.split('/').count() == 2
                    && (name.ends_with(".png") || name.ends_with(".ts")))
                || matches!(
                    name,
                    "coins/keywords.csv"
                        | "README.md"
                        | "CHANGELOG.md"
                        | "LICENSE"
                        | ".github/CONTRIBUTING.md"
                        | "tests/README.md"
                        | "tests/oracles/README.md"
                        | "tests/reference/README.md"
                        | "tests/reference/consumer-provenance.md"
                        | "native/data/python311-decimal.json"
                        | "native/data/python311-nonprintable.json"
                )
                || (name.starts_with("docs/") && name.ends_with(".md"))
                || (name.starts_with(".github/") && name.ends_with(".md"))
                || (name.starts_with("tests/") && name.ends_with(".md"))
                || name == "CONTEXT.md"
                || (name.starts_with("worker/")
                    && [".ts", ".json", ".toml", ".sql", ".md", ".ps1"]
                        .iter()
                        .any(|suffix| name.ends_with(suffix)))
        }
        "config" => {
            matches!(
                name,
                "native/Cargo.toml"
                    | "native/Cargo.lock"
                    | ".cargo/tests.toml"
                    | ".env.example"
                    | "pyproject.toml"
                    | "ruff.toml"
                    | ".github/dependabot.yml"
            ) || (!name.contains('/') && name.starts_with("requirements") && name.ends_with(".txt"))
                || ((name.starts_with(".github/workflows/")
                    || name.starts_with(".github/actions/"))
                    && (name.ends_with(".yml") || name.ends_with(".yaml")))
        }
        _ => false,
    };
    if !public_path(name) || !allowed {
        return Err(refusal(
            "closure role/path is unsafe, protected or outside public input policy",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_powershell_source_paths_do_not_admit_other_root_or_protected_scripts() {
        for name in [
            "run.ps1",
            "scripts/run-actions.ps1",
            "tests/test_original.py",
        ] {
            assert!(member_path("source", name).is_ok(), "{name}");
        }
        for name in [
            "other-root.ps1",
            "private/run.ps1",
            ".claude/hooks/x.ps1",
            "scripts/private/x.ps1",
            "scripts/../run.ps1",
            "tests/other.ps1",
        ] {
            assert!(member_path("source", name).is_err(), "{name}");
        }
    }
    #[test]
    fn public_consumed_assets_and_documents_keep_protected_and_unrelated_files_out() {
        for name in [
            "assets/logo.png",
            "assets/panel.ts",
            "coins/keywords.csv",
            "README.md",
            "CHANGELOG.md",
            "LICENSE",
            "docs/GUIDE.md",
            ".github/CONTRIBUTING.md",
            ".github/ISSUE_TEMPLATE/bug_report.md",
            ".github/SECURITY.md",
            "CONTEXT.md",
            "worker/src/index.ts",
            "worker/package-lock.json",
            "worker/migrations/0001_logs.sql",
            "tests/README.md",
            "tests/oracles/README.md",
            "tests/reference/README.md",
            "tests/reference/consumer-provenance.md",
            "tests/other.md",
            "native/data/python311-decimal.json",
            "native/data/python311-nonprintable.json",
        ] {
            assert!(member_path("support", name).is_ok(), "{name}");
        }
        for name in ["coins/run_convert.ps1", "coins/run_rebuild_loop.ps1"] {
            assert!(member_path("source", name).is_ok(), "{name}");
        }
        for name in [
            "assets/nested/logo.png",
            "assets/logo.ico",
            "assets/logo.svg",
            "assets/private/logo.png",
            "coins/other.csv",
            "other.md",
            "tests/private/README.md",
            "docs/private/note.md",
            "native/data/other.json",
            "native/data/private/key.json",
        ] {
            assert!(member_path("support", name).is_err(), "{name}");
        }
        assert!(member_path("source", "coins/other.ps1").is_err());
        assert!(member_path("config", ".env.example").is_ok());
        for role in ["source", "target", "fixture", "support"] {
            assert!(member_path(role, ".env.example").is_err());
        }
        for name in [
            ".env",
            ".env.local",
            ".ENV.EXAMPLE",
            ".env.example.key",
            ".env.example/other",
            "private/.env.example",
            "nested/.env.example",
            "./.env.example",
            "../.env.example",
        ] {
            assert!(!public_path(name), "{name}");
            assert!(member_path("config", name).is_err(), "{name}");
        }
    }
}
