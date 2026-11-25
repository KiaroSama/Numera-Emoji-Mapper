use numera_runtime::package::{verify_binary_version, verify_binary_version_bytes};
use std::collections::BTreeMap;

#[path = "support/test_admission.rs"]
mod test_admission;

fn word(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn dword(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn pad(bytes: &mut Vec<u8>) {
    while !bytes.len().is_multiple_of(4) {
        bytes.push(0);
    }
}

// Independent VERSIONINFO writer follows the documented WORD lengths and DWORD
// sibling alignment; no production parser or packing helper constructs the oracle.
fn block(key: &str, kind: u16, value: &[u8], children: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = vec![0; 6];
    word(
        &mut bytes,
        2,
        (if kind == 1 {
            value.len() / 2
        } else {
            value.len()
        })
        .try_into()
        .unwrap(),
    );
    word(&mut bytes, 4, kind);
    bytes.extend(utf16(key));
    pad(&mut bytes);
    bytes.extend(value);
    for child in children {
        pad(&mut bytes);
        bytes.extend(child);
    }
    let length = bytes.len().try_into().unwrap();
    word(&mut bytes, 0, length);
    bytes
}

fn fields() -> [u16; 4] {
    let mut components = env!("CARGO_PKG_VERSION").split(['.', '-', '+']);
    [
        components.next().unwrap().parse().unwrap(),
        components.next().unwrap().parse().unwrap(),
        components.next().unwrap().parse().unwrap(),
        0,
    ]
}

fn fixed(file: [u16; 4], product: [u16; 4]) -> Vec<u8> {
    let mut bytes = vec![0; 52];
    dword(&mut bytes, 0, 0xfeef04bd);
    dword(&mut bytes, 4, 0x10000);
    for (offset, tuple) in [(8, file), (16, product)] {
        // Write the independent little-endian Windows WORD order directly.
        for (index, component) in [tuple[1], tuple[0], tuple[3], tuple[2]]
            .into_iter()
            .enumerate()
        {
            word(&mut bytes, offset + index * 2, component);
        }
    }
    bytes
}

fn strings(file: &str, product: &str) -> Vec<Vec<u8>> {
    vec![
        block("FileVersion", 1, &utf16(file), &[]),
        // This value ends before its following sibling's DWORD alignment.
        block(
            "LegalCopyright",
            1,
            &utf16("KiaroSama (GPL-3.0-or-later)"),
            &[],
        ),
        block("ProductVersion", 1, &utf16(product), &[]),
    ]
}

fn resource(file: [u16; 4], product: [u16; 4], tables: &[Vec<u8>]) -> Vec<u8> {
    // The ordinary fixture advertises only its neutral table. Negative tests
    // must not fail because of an unrelated absent language table.
    resource_with_translations(file, product, tables, Some(&[(0, 0x04b0)]))
}

fn translation_info(pairs: &[(u16, u16)]) -> Vec<u8> {
    let bytes = pairs
        .iter()
        .flat_map(|(language, codepage)| {
            language
                .to_le_bytes()
                .into_iter()
                .chain(codepage.to_le_bytes())
        })
        .collect::<Vec<_>>();
    block(
        "VarFileInfo",
        1,
        &[],
        &[block("Translation", 0, &bytes, &[])],
    )
}

fn resource_with_translations(
    file: [u16; 4],
    product: [u16; 4],
    tables: &[Vec<u8>],
    pairs: Option<&[(u16, u16)]>,
) -> Vec<u8> {
    let mut children = vec![block("StringFileInfo", 1, &[], tables)];
    if let Some(pairs) = pairs {
        children.push(translation_info(pairs));
    }
    block("VS_VERSION_INFO", 0, &fixed(file, product), &children)
}

fn valid_resource() -> Vec<u8> {
    resource(
        fields(),
        fields(),
        &[
            block(
                "000004b0",
                1,
                &[],
                &strings(env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_VERSION")),
            ),
            block(
                "040904B0",
                1,
                &[],
                &strings(env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_VERSION")),
            ),
        ],
    )
}

fn directory(bytes: &mut [u8], offset: usize, entries: &[(u32, u32)]) {
    word(bytes, offset + 14, entries.len().try_into().unwrap());
    for (index, &(id, target)) in entries.iter().enumerate() {
        dword(bytes, offset + 16 + index * 8, id);
        dword(bytes, offset + 20 + index * 8, target);
    }
}

fn pe(machine: u16, versions: &[(u32, u32, Vec<u8>)]) -> Vec<u8> {
    let mut names = BTreeMap::<u32, Vec<(u32, &Vec<u8>)>>::new();
    for (id, language, payload) in versions {
        names.entry(*id).or_default().push((*language, payload));
    }
    let names_offset = 24;
    let mut resources = vec![0; names_offset + 16 + names.len() * 8];
    directory(
        &mut resources,
        0,
        &[(16, 0x8000_0000 | names_offset as u32)],
    );
    let mut name_entries = Vec::new();
    for (id, mut languages) in names {
        languages.sort_by_key(|entry| entry.0);
        let offset = resources.len();
        resources.resize(offset + 16 + languages.len() * 8, 0);
        name_entries.push((id, 0x8000_0000 | offset as u32));
        let mut language_entries = Vec::new();
        for (language, payload) in languages {
            pad(&mut resources);
            let data = resources.len();
            resources.resize(data + 16, 0);
            language_entries.push((language, data as u32));
            let payload_offset = resources.len();
            resources.extend(payload);
            dword(&mut resources, data, 0x1000 + payload_offset as u32);
            dword(&mut resources, data + 4, payload.len() as u32);
        }
        directory(&mut resources, offset, &language_entries);
        pad(&mut resources);
    }
    directory(&mut resources, names_offset, &name_entries);
    let mut bytes = vec![0; 512];
    bytes[..2].copy_from_slice(b"MZ");
    dword(&mut bytes, 0x3c, 0x80);
    bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
    word(&mut bytes, 0x84, machine);
    word(&mut bytes, 0x86, 1);
    word(&mut bytes, 0x94, 240);
    word(&mut bytes, 0x98, 0x20b);
    dword(&mut bytes, 0x98 + 108, 16);
    dword(&mut bytes, 0x98 + 128, 0x1000);
    dword(&mut bytes, 0x98 + 132, resources.len() as u32);
    let section = 0x98 + 240;
    bytes[section..section + 8].copy_from_slice(b".rsrc\0\0\0");
    dword(&mut bytes, section + 12, 0x1000);
    dword(&mut bytes, section + 16, resources.len() as u32);
    dword(&mut bytes, section + 20, 512);
    bytes.extend(resources);
    bytes
}

fn check(payload: Vec<u8>) -> Result<(), String> {
    verify_binary_version_bytes(&pe(0x8664, &[(1, 1033, payload)]))
}

#[test]
fn complete_x64_and_arm64_multilingual_versions_are_accepted() {
    for machine in [0x8664, 0xaa64] {
        let bytes = pe(
            machine,
            &[
                (1, 0, valid_resource()),
                (1, 1033, valid_resource()),
                (2, 1033, valid_resource()),
            ],
        );
        verify_binary_version_bytes(&bytes).unwrap();
    }
}

#[test]
fn every_advertised_translation_must_resolve_to_a_validated_table() {
    let table = block(
        "000004b0",
        1,
        &[],
        &strings(env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_VERSION")),
    );
    check(resource(fields(), fields(), std::slice::from_ref(&table))).unwrap();
    for pairs in [
        vec![(0x0411, 0x04b0)],
        vec![(0, 0x04b0), (0x0411, 0x04b0)],
        vec![(0, 0x04b0), (0, 0x04e4)],
    ] {
        let error = check(resource_with_translations(
            fields(),
            fields(),
            std::slice::from_ref(&table),
            Some(&pairs),
        ))
        .unwrap_err();
        assert!(
            error.contains("Translation refers to a missing string table"),
            "{error}"
        );
    }
}

#[test]
fn optional_translations_and_valid_multilingual_tables_remain_accepted() {
    let tables = [
        block(
            "000004b0",
            1,
            &[],
            &strings(env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_VERSION")),
        ),
        block(
            "040904B0",
            1,
            &[],
            &strings(env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_VERSION")),
        ),
    ];
    for pairs in [
        None,
        Some(&[(0, 0x04b0)][..]),
        Some(&[(0, 0x04b0), (0x0409, 0x04b0)][..]),
    ] {
        check(resource_with_translations(
            fields(),
            fields(),
            &tables,
            pairs,
        ))
        .unwrap();
    }
    // Advertised tables need not match the PE resource's language, and the
    // optional information blocks can be resolved after either sibling order.
    let var_first = block(
        "VS_VERSION_INFO",
        0,
        &fixed(fields(), fields()),
        &[
            translation_info(&[(0, 0x04b0), (0x0409, 0x04b0)]),
            block("StringFileInfo", 1, &[], &tables),
        ],
    );
    check(var_first).unwrap();
}

#[test]
fn every_file_and_product_numeric_component_is_checked() {
    for field in 0..2 {
        for component in 0..4 {
            let mut file = fields();
            let mut product = fields();
            let selected = if field == 0 { &mut file } else { &mut product };
            selected[component] ^= 1;
            let payload = resource(
                file,
                product,
                &[block(
                    "000004b0",
                    1,
                    &[],
                    &strings(env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_VERSION")),
                )],
            );
            let error = check(payload).unwrap_err();
            assert!(
                error.contains("numeric FileVersion or ProductVersion differs"),
                "field {field}, component {component}: {error}"
            );
        }
    }
}

#[test]
fn a_later_resource_language_or_identity_cannot_be_ignored() {
    for (id, language) in [(1, 1033), (2, 0)] {
        let bad = resource(
            [0, 0, 0, 0],
            fields(),
            &[block(
                "000004b0",
                1,
                &[],
                &strings(env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_VERSION")),
            )],
        );
        assert!(
            verify_binary_version_bytes(&pe(
                0x8664,
                &[(1, 0, valid_resource()), (id, language, bad)]
            ))
            .is_err()
        );
    }
}

#[test]
fn every_string_table_requires_both_unique_exact_version_strings() {
    let good = strings(env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_VERSION"));
    check(resource(
        fields(),
        fields(),
        &[
            block("000004b0", 1, &[], &good),
            block("040904b0", 1, &[], &good),
        ],
    ))
    .unwrap();
    let mut missing = good.clone();
    missing.pop();
    let mut duplicate = good.clone();
    duplicate.push(good[0].clone());
    for bad in [
        strings("0.0.0", env!("CARGO_PKG_VERSION")),
        strings(env!("CARGO_PKG_VERSION"), "0.0.0"),
        strings("", ""),
        missing,
        duplicate,
        Vec::new(),
    ] {
        assert!(
            check(resource(
                fields(),
                fields(),
                &[
                    block("000004b0", 1, &[], &good),
                    block("040904b0", 1, &[], &bad),
                ]
            ))
            .is_err()
        );
    }
}

#[test]
fn malformed_utf16_lengths_types_and_string_children_are_rejected() {
    for bad in [
        block("FileVersion", 1, &[0, 0xd8, 0, 0], &[]),
        block("FileVersion", 1, &[b'3', 0], &[]),
        block("FileVersion", 1, &[0, 0, b'3', 0, 0, 0], &[]),
        block("FileVersion", 0, &utf16(env!("CARGO_PKG_VERSION")), &[]),
        block(
            "FileVersion",
            1,
            &utf16(env!("CARGO_PKG_VERSION")),
            &[block("Unexpected", 1, &utf16("child"), &[])],
        ),
    ] {
        assert!(
            check(resource(
                fields(),
                fields(),
                &[block(
                    "000004b0",
                    1,
                    &[],
                    &[
                        bad,
                        block("ProductVersion", 1, &utf16(env!("CARGO_PKG_VERSION")), &[]),
                    ]
                )]
            ))
            .is_err()
        );
    }
    let mut bad_length = valid_resource();
    word(&mut bad_length, 2, 51);
    assert!(check(bad_length).is_err());
}

#[test]
fn missing_duplicate_and_malformed_version_containers_are_rejected() {
    let table = block(
        "000004b0",
        1,
        &[],
        &strings(env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_VERSION")),
    );
    let info = block("StringFileInfo", 1, &[], std::slice::from_ref(&table));
    for payload in [
        block("VS_VERSION_INFO", 0, &fixed(fields(), fields()), &[]),
        block(
            "VS_VERSION_INFO",
            0,
            &fixed(fields(), fields()),
            &[info.clone(), info],
        ),
        resource(fields(), fields(), &[table.clone(), table]),
        resource(
            fields(),
            fields(),
            &[block(
                "not-hex!",
                1,
                &[],
                &strings(env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_VERSION")),
            )],
        ),
        block("Unexpected", 0, &fixed(fields(), fields()), &[]),
        block("VS_VERSION_INFO", 1, &fixed(fields(), fields()), &[]),
    ] {
        assert!(check(payload).is_err());
    }
    for offset in [40, 44] {
        let mut payload = valid_resource();
        payload[offset] ^= 1;
        assert!(check(payload).is_err());
    }
}

#[test]
fn truncated_resources_and_invalid_resource_ranges_fail_closed() {
    let original = pe(0x8664, &[(1, 1033, valid_resource())]);
    for length in 0..original.len() {
        assert!(
            verify_binary_version_bytes(&original[..length]).is_err(),
            "prefix {length}"
        );
    }
    let mut missing = original.clone();
    dword(&mut missing, 512 + 16, 15);
    assert!(verify_binary_version_bytes(&missing).is_err());
    let mut overflow = original.clone();
    dword(&mut overflow, 0x98 + 128, u32::MAX);
    assert!(verify_binary_version_bytes(&overflow).is_err());
    let mut zero_child = valid_resource();
    word(&mut zero_child, 92, 0);
    assert!(check(zero_child).is_err());
    let mut trailing = valid_resource();
    trailing.extend([0, 0, 0, 0]);
    assert!(check(trailing).is_err());
}

#[test]
fn public_gate_uses_compiled_version_instead_of_the_target_claim() {
    let wrong = resource(
        [0, 0, 0, 0],
        [0, 0, 0, 0],
        &[block("000004b0", 1, &[], &strings("0.0.0", "0.0.0"))],
    );
    assert!(check(wrong).is_err());
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("version [literal] file.exe");
    let bytes = pe(0xaa64, &[(1, 1033, valid_resource())]);
    std::fs::write(&path, &bytes).unwrap();
    verify_binary_version(&path).unwrap();
    let command = |args: Vec<String>| {
        numera_runtime::package::cli::run(root.path(), "verify-binary-version", &args)
    };
    assert_eq!(
        command(vec!["--executable".into(), path.to_str().unwrap().into()]),
        0
    );
    assert_eq!(
        command(vec!["--executable=version [literal] file.exe".into()]),
        0
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert!(verify_binary_version(&root.path().join("missing.exe")).is_err());
}

#[test]
fn version_cli_rejects_missing_unknown_empty_and_duplicate_arguments() {
    let root = tempfile::tempdir().unwrap();
    let command = |args: &[&str]| {
        numera_runtime::package::cli::run(
            root.path(),
            "verify-binary-version",
            &args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
        )
    };
    assert_eq!(command(&["--help"]), 0);
    for args in [
        vec![],
        vec!["--executable"],
        vec!["--executable="],
        vec!["--executable", "--unknown"],
        vec!["--unknown", "file.exe"],
        vec!["--executable", "file.exe", "--executable", "second.exe"],
        vec!["--executable", "missing.exe"],
        vec!["--expected-version", "3.1.0"],
    ] {
        assert_eq!(command(&args), 2, "{args:?}");
    }
}
