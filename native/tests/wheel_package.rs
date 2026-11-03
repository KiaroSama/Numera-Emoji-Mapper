//! Real built-wheel acceptance, not the synthetic source-schema1 package checkpoint.
use std::io::{Cursor, Read, Seek, Write};
use std::path::Path;

const PRIVATE_DIRECTORIES: &[&str] = &[
    "tests",
    "private",
    ".ai",
    ".claude",
    ".kiro",
    ".codex",
    ".agents",
    ".specify",
    "specs",
    "plans",
    "logs",
    "collection",
    "packs",
    "graphify-out",
    ".codebase-memory",
];

fn stored_wheel(names: &[&str]) -> Vec<u8> {
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for name in names {
        archive.start_file(*name, options).unwrap();
        archive.write_all(b"synthetic wheel member").unwrap();
    }
    archive.finish().unwrap().into_inner()
}

#[test]
fn wheel_assertions_reject_each_original_private_class_missing_extension_and_crc_damage() {
    let extension = "emojikit/_native.abi3.so";
    for extension in [extension, "emojikit/_native.cp311-win_amd64.pyd"] {
        let bytes = stored_wheel(&[extension, "emojikit/public.txt"]);
        assert_eq!(verify_wheel(Cursor::new(bytes)).unwrap(), 2);
    }
    let mut forbidden = PRIVATE_DIRECTORIES
        .iter()
        .map(|part| format!("emojikit/{part}/payload"))
        .collect::<Vec<_>>();
    forbidden.extend([
        "emojikit/SeCrEtS.MD".into(),
        "emojikit/EXPLAIN-AI.md".into(),
        "emojikit/.IgNoReMe".into(),
        "emojikit/.env".into(),
        "emojikit/.env.local".into(),
    ]);
    for name in forbidden {
        let bytes = stored_wheel(&[extension, &name]);
        let error = verify_wheel(Cursor::new(bytes)).unwrap_err();
        assert!(error.contains("private wheel member"), "{name}: {error}");
    }
    for names in [
        vec![],
        vec!["emojikit/public.txt"],
        vec!["other/_native.so"],
    ] {
        let bytes = stored_wheel(&names);
        assert!(
            verify_wheel(Cursor::new(bytes))
                .unwrap_err()
                .contains("native extension")
        );
    }
    let mut bytes = stored_wheel(&[extension, "emojikit/public.txt"]);
    let position = {
        let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
        let member = archive.by_index(1).unwrap();
        usize::try_from(member.data_start().expect("opened member data offset")).unwrap()
    };
    bytes[position] ^= 1;
    // Parsing the central directory alone does not preserve ZipFile.testzip.
    assert!(zip::ZipArchive::new(Cursor::new(&bytes)).is_ok());
    assert!(
        verify_wheel(Cursor::new(bytes))
            .unwrap_err()
            .contains("CRC/header")
    );
}

#[test]
fn actual_built_wheel_excludes_original_private_content_and_validates_every_member() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let directory = root.join("native/target/wheels");
    _native::tooling_environment::reject_links(&directory, "wheel directory cannot follow links")
        .unwrap();
    let mut wheels = std::fs::read_dir(&directory)
        .expect("build the required wheel before packaging acceptance")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|suffix| suffix == "whl"))
        .map(|path| {
            _native::tooling_environment::reject_links(&path, "wheel cannot follow links").unwrap();
            let metadata = std::fs::metadata(&path).unwrap();
            assert!(metadata.is_file(), "wheel must be a regular file");
            (metadata.modified().unwrap(), path)
        })
        .collect::<Vec<_>>();
    wheels.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let (_, wheel) = wheels
        .last()
        .expect("build the required wheel before packaging acceptance");
    let members = verify_wheel(std::fs::File::open(wheel).unwrap())
        .unwrap_or_else(|error| panic!("{}: {error}", wheel.display()));
    assert!(members > 0);
    println!(
        "Validated built wheel: {} ({members} members)",
        wheel.display()
    );
}

fn verify_wheel(reader: impl Read + Seek) -> Result<usize, String> {
    let mut archive = zip::ZipArchive::new(reader).map_err(|error| error.to_string())?;
    let mut native = false;
    for index in 0..archive.len() {
        let mut member = archive.by_index(index).map_err(|error| error.to_string())?;
        let name = member.name().to_owned();
        let basename = name
            .split('/')
            .rfind(|part| !part.is_empty() && *part != ".")
            .unwrap_or("");
        if name
            .split('/')
            .any(|part| PRIVATE_DIRECTORIES.contains(&part))
            || matches!(
                basename.to_lowercase().as_str(),
                "secrets.md" | "explain-ai.md" | ".ignoreme"
            )
            || basename.starts_with(".env")
        {
            return Err(format!("private wheel member at index {index}"));
        }
        native |= name.starts_with("emojikit/_native")
            && (name.ends_with(".so") || name.ends_with(".pyd"));
        // Reaching EOF through the ordinary reader verifies CRC and local headers.
        std::io::copy(&mut member, &mut std::io::sink()).map_err(|error| {
            format!("wheel CRC/header verification failed at index {index}: {error}")
        })?;
    }
    if !native {
        return Err("built wheel contains no native extension".into());
    }
    Ok(archive.len())
}
