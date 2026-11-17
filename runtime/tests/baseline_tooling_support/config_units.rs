//! Preserve the original whole-dictionary BOM assertion without a public test getter.
#![allow(non_snake_case)]
mod ADotEnvWithAByteOrderMark {
    // Original: tests.test_cli_env.ADotEnvWithAByteOrderMark.test_the_first_key_is_read_under_its_own_name
    #[test]
    fn test_the_first_key_is_read_under_its_own_name() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("logs/tmp")
            .join(format!("tooling-config-bom-{}", std::process::id()));
        std::fs::create_dir_all(root.parent().unwrap()).unwrap();
        std::fs::create_dir(&root).unwrap();
        std::fs::write(
            root.join(".env"),
            "\u{feff}FIRST_KEY=one\n# note\nSECOND='two'\n".as_bytes(),
        )
        .unwrap();
        let config = crate::config::Config::load_with_environment(
            root.clone(),
            &std::collections::BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(
            config.values,
            std::collections::BTreeMap::from([
                ("FIRST_KEY".into(), "one".into()),
                ("SECOND".into(), "two".into())
            ])
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
