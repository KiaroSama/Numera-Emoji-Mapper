use super::*;
// tests.test_cli_docs.TheCliReferenceMatchesTheTools.test_the_scan_finds_the_sections
#[test]
fn test_the_scan_finds_the_sections() {
    let sections = sections();
    assert_eq!(sections["12.2"].0, BTreeSet::from(["build-pack".into()]));
    assert_eq!(sections["12.8"].0.len(), 14);
    assert_eq!(
        sections["12.8"].0,
        [
            "coin-convert",
            "coin-rebuild-loop",
            "coin-fetch-logos",
            "coin-build-keywords",
            "coin-fetch-paprika",
            "coin-fetch-cmc",
            "coin-rebuild",
            "coin-rebuild-map",
            "coin-remap-ids",
            "coin-verify-logos",
            "coin-check-packs",
            "coin-write-manifests",
            "coin-enhance-map",
            "coin-alias-map"
        ]
        .map(str::to_owned)
        .into_iter()
        .collect()
    );
}
// tests.test_cli_docs.TheCliReferenceMatchesTheTools.test_every_flag_is_documented_and_every_documented_flag_exists
#[test]
fn test_every_flag_is_documented_and_every_documented_flag_exists() {
    let root = Root::new();
    root.install();
    for (id, (tools, body)) in sections() {
        if tools.is_empty() {
            continue;
        }
        let mut actual = BTreeSet::new();
        for command in tools {
            let result = root.run(&[&command, "--help"], &[]);
            assert_eq!(result.code, 0, "{command}: {}", result.stderr);
            actual.extend(flags(&result.stdout));
        }
        actual.remove("--help");
        let mut documented = flags(&body);
        documented.remove("--help");
        if id == "12.6" && body.contains("numera-dev panel-sandbox") {
            // This section describes two distinct real commands. Source
            // and the bounded lifetime belong to the developer wrapper,
            // not the production panel or the flag-free polling bot.
            let result = root.dev(&["--help"]);
            assert_eq!(result.code, 0, "{}", result.stderr);
            assert!(result.stdout.contains("panel-sandbox"));
            let wrapper = flags(&result.stdout);
            for flag in ["--source", "--timeout-seconds"] {
                assert!(
                    documented.contains(flag),
                    "§{id} missing wrapper documentation for {flag}"
                );
                assert!(
                    wrapper.contains(flag),
                    "§{id} documented wrapper flag absent from actual developer help: {flag}"
                );
                documented.remove(flag);
            }
        }
        let mentioned = match id.as_str() {
            "12.6" => vec!["--new-set", "--per-set"],
            "12.5e" => vec!["--check"],
            "12.5d" => vec!["--refresh"],
            _ => vec![],
        }
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
        assert_eq!(
            actual.difference(&documented).collect::<Vec<_>>(),
            Vec::<&String>::new(),
            "§{id} undocumented flags"
        );
        assert!(
            documented
                .difference(&actual)
                .all(|flag| mentioned.contains(flag)),
            "§{id} stale flags: {:?}",
            documented.difference(&actual).collect::<Vec<_>>()
        );
    }
}
