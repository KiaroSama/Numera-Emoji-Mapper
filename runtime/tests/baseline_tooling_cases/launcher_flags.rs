use super::*;

mod TheLauncherPassesOnlyRealFlags {
    use super::*;
    // tests.test_launcher_contract.TheLauncherPassesOnlyRealFlags.test_the_scan_finds_the_launchers_modules
    #[test]
    fn test_the_scan_finds_the_launchers_modules() {
        let root = Root::new();
        root.install();
        let actual = launcher_requests(&root);
        for command in [
            "build-pack",
            "build-collection",
            "panel",
            "fetch-pack",
            "fetch-emoji-ids",
            "sync-order",
            "pack-manifest",
            "pack-archive",
            "status",
            "make-emoji-pngs",
            "add-media",
            "emoji-bot",
        ] {
            assert!(
                actual.contains_key(command),
                "{command} missing from actual launcher actions"
            );
        }
        assert!(actual["panel"].contains("--with-pack"));
        assert!(actual["fetch-pack"].contains("--token-env"));
        assert!(actual.len() >= 13);
        let observed_flags = actual
            .values()
            .flat_map(|flags| flags.iter().cloned())
            .collect::<BTreeSet<_>>();
        let source_flags = launcher_source_flags();
        assert!(!source_flags.is_empty());
        assert_eq!(
            observed_flags, source_flags,
            "actual menu probes must exercise every flag literal in its production builders"
        );
        let modules = source("runtime/src/lib.rs");
        assert!(
            modules.items.iter().any(
                |item| matches!(item,syn::Item::Mod(module)if module.ident=="tooling_menu_cli")
            )
        );
        let menu = root.run(&["menu"], &[]);
        assert_eq!(menu.code, 0, "{}", menu.stderr);
        assert!(menu.stdout.contains("Bye."));
        let doctor = root.run(&["doctor"], &[]);
        assert_eq!(doctor.code, 0, "{}", doctor.stderr);
        assert!(doctor.stdout.contains("Environment check complete"));
    }
    // tests.test_launcher_contract.TheLauncherPassesOnlyRealFlags.test_every_flag_is_in_its_modules_help
    #[test]
    fn test_every_flag_is_in_its_modules_help() {
        let root = Root::new();
        root.install();
        for (command, flags) in launcher_requests(&root) {
            if flags.is_empty() {
                continue;
            }
            let result = root.run(&[&command, "--help"], &[]);
            assert_eq!(result.code, 0, "{command}: {}", result.stderr);
            for flag in flags {
                assert!(
                    result.stdout.contains(&flag),
                    "launcher passed unsupported {command} {flag}"
                );
            }
        }
    }
    // tests.test_launcher_contract.TheLauncherPassesOnlyRealFlags.test_the_coin_rebuild_default_command_still_exists
    #[test]
    fn test_the_coin_rebuild_default_command_still_exists() {
        let root = Root::new();
        root.install();
        assert!(launcher_requests(&root).contains_key("coin-rebuild"));
        let result = root.run(&["coin-rebuild", "--help"], &[]);
        assert_eq!(result.code, 0, "{}", result.stderr);
        assert!(result.stdout.contains("all"));
    }
}
