use numera_runtime as runtime;
use runtime::{
    catalog::Catalog, logging::RunLog, tooling_environment, tooling_process, tooling_sandbox,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use syn::visit::Visit;

mod live;
pub use live::*;

pub const UTC: &str = "2026-01-01T00:00:00Z";
pub const EMPTY_SVG: &str =
    "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"64\"></svg>";
static NEXT: AtomicU64 = AtomicU64::new(0);
pub fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .into()
}
pub fn executor() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
pub struct Root(pub PathBuf);
impl Root {
    pub fn new() -> Self {
        let base = tooling_environment::confined_directory(&project(), "logs/tmp", true).unwrap();
        let path = base.join(format!(
            "baseline tooling café {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(tooling_environment::physical_root(&path).unwrap())
    }
    pub fn file(&self, name: impl AsRef<Path>, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        path
    }
    pub fn install(&self) {
        for (source, name) in [
            (env!("CARGO_BIN_EXE_numera-emoji"), app_name()),
            (env!("CARGO_BIN_EXE_numera-dev"), dev_name()),
        ] {
            std::fs::copy(source, self.0.join(name)).unwrap();
        }
        for name in ["panel.html", "logo-128.png"]
            .into_iter()
            .chain(runtime::asset_version::PANEL_SCRIPTS.iter().copied())
        {
            self.file(
                format!("assets/{name}"),
                &std::fs::read(project().join("assets").join(name)).unwrap(),
            );
        }
        self.file(".env.example", b"GENERAL_BOT_TOKEN=\nPACK_OWNER_USER_ID=\n");
        self.file(
            "runtime/Cargo.toml",
            &std::fs::read(project().join("runtime/Cargo.toml")).unwrap(),
        );
    }
    pub fn source(&self, keys: &[&str], outside: bool) -> PathBuf {
        let source = self.0.join("source");
        std::fs::create_dir_all(source.join("media")).unwrap();
        let mut cat = Catalog::open(&source.join("catalog.db"), UTC, &self.0).unwrap();
        for (index, key) in keys.iter().enumerate() {
            let directory = if outside { "outside" } else { "source/media" };
            let mut body = b"\x89PNG-".to_vec();
            body.extend_from_slice(index.to_string().as_bytes());
            let path = self.file(format!("{directory}/{index}.png"), &body);
            assert!(
                cat.insert(
                    &json!({"content_key":key,"fmt":"static","file_path":path}),
                    UTC
                )
                .unwrap()
            );
        }
        drop(cat);
        source
    }
    pub fn destination(&self) -> PathBuf {
        self.0.join("panel-sandbox-dest")
    }
    pub fn clone(&self, source: &Path) -> tooling_sandbox::Session {
        tooling_sandbox::Session::create(&self.0, source, &self.destination()).unwrap()
    }
    pub fn raster(&self, relative: &str, color: [u8; 4]) -> PathBuf {
        let image = image::RgbaImage::from_pixel(40, 70, image::Rgba(color));
        self.file(
            relative,
            &runtime::native_raster::encode_png(&image).unwrap(),
        )
    }
    pub fn run(&self, args: &[&str], extra: &[(&str, String)]) -> tooling_process::Completed {
        self.run_at(&self.0, args, extra)
    }
    pub fn run_at(
        &self,
        cwd: &Path,
        args: &[&str],
        extra: &[(&str, String)],
    ) -> tooling_process::Completed {
        self.program(app_name(), cwd, args, extra)
    }
    pub fn dev(&self, args: &[&str]) -> tooling_process::Completed {
        self.program(dev_name(), &self.0, args, &[])
    }
    fn program(
        &self,
        name: &str,
        cwd: &Path,
        args: &[&str],
        extra: &[(&str, String)],
    ) -> tooling_process::Completed {
        let mut argv = vec![self.0.join(name).into_os_string()];
        argv.extend(args.iter().map(std::ffi::OsString::from));
        let mut environment = machine();
        environment.extend(
            extra
                .iter()
                .map(|(key, value)| ((*key).into(), value.clone())),
        );
        let environment = tooling_environment::environment(cwd, &environment, false).unwrap();
        let log = RunLog::open(cwd, "tooling-baseline-child", vec![]);
        let result = executor()
            .block_on(tooling_process::capture(
                cwd,
                &argv,
                &environment,
                Duration::from_secs(30),
                Duration::from_secs(15),
                &log,
            ))
            .unwrap();
        log.finish(result.code);
        result
    }
    pub fn menu(&self, input: &str, codes: &[i32]) -> Vec<runtime::tooling_menu::Request> {
        let mut seen = vec![];
        runtime::tooling_menu::run(
            &self.0,
            &mut Cursor::new(input.as_bytes()),
            &mut Vec::new(),
            false,
            |request| {
                let code = *codes.get(seen.len()).unwrap_or(&0);
                seen.push(request.clone());
                Ok(code)
            },
        )
        .unwrap();
        seen
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("tooling evidence retained: {}", self.0.display());
        } else {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
pub fn app_name() -> &'static str {
    if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    }
}
pub fn dev_name() -> &'static str {
    if cfg!(windows) {
        "numera-dev.exe"
    } else {
        "numera-dev"
    }
}
pub fn machine() -> BTreeMap<String, String> {
    let mut env = BTreeMap::from([
        ("PATH".into(), String::new()),
        ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
    ]);
    for key in ["SystemRoot", "WINDIR", "LD_LIBRARY_PATH"] {
        if let Ok(value) = std::env::var(key) {
            env.insert(key.into(), value);
        }
    }
    env
}
pub fn paths(directory: &Path) -> Vec<PathBuf> {
    let db = rusqlite::Connection::open_with_flags(
        directory.join("catalog.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    db.prepare("SELECT file_path FROM items ORDER BY content_key")
        .unwrap()
        .query_map([], |row| {
            row.get::<_, String>(0).map(|stored| {
                if let Some(relative) = stored.strip_prefix("./") {
                    directory.join(relative)
                } else if Path::new(&stored).is_absolute() {
                    PathBuf::from(stored)
                } else {
                    directory.parent().unwrap().join(stored)
                }
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}
pub fn abandoned(root: &Root, name: &str, kind: &str) -> PathBuf {
    let path = root.0.join(format!("panel-sandbox-{name}"));
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("catalog.db"), b"data").unwrap();
    match kind {
        "unmarked" => {}
        "malformed" => {
            std::fs::write(path.join(".sandbox-owner.json"), b"{not json").unwrap();
        }
        _ => {
            let directory = if kind == "foreign" {
                root.0.join("elsewhere")
            } else {
                path.clone()
            };
            std::fs::write(
                path.join(".sandbox-owner.json"),
                json!({"version":2,"directory":directory}).to_string(),
            )
            .unwrap();
        }
    }
    path
}
#[cfg(unix)]
pub fn storage_identity(path: &Path) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    let metadata = path.metadata().unwrap();
    (metadata.dev(), metadata.ino())
}
#[cfg(windows)]
pub fn storage_identity(path: &Path) -> (u64, u64) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let file = std::fs::File::open(path).unwrap();
    unsafe {
        let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        assert_ne!(
            GetFileInformationByHandle(file.as_raw_handle(), &mut info),
            0
        );
        (
            u64::from(info.dwVolumeSerialNumber),
            (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        )
    }
}
pub fn source(file: &str) -> syn::File {
    syn::parse_file(&std::fs::read_to_string(project().join(file)).unwrap()).unwrap()
}
struct Strings(BTreeSet<String>);
impl<'ast> Visit<'ast> for Strings {
    fn visit_lit_str(&mut self, node: &'ast syn::LitStr) {
        self.0.insert(node.value());
    }
}
pub fn literals(node: &syn::Expr) -> BTreeSet<String> {
    let mut visitor = Strings(BTreeSet::new());
    visitor.visit_expr(node);
    visitor.0
}
pub fn launcher_source_flags() -> BTreeSet<String> {
    let parsed =
        syn::parse_file(&std::fs::read_to_string(owned_module("tooling_menu_actions")).unwrap())
            .unwrap();
    let mut visitor = Strings(BTreeSet::new());
    visitor.visit_file(&parsed);
    visitor
        .0
        .into_iter()
        .filter(|value| value.starts_with("--"))
        .collect()
}
pub fn commands() -> BTreeSet<String> {
    let file = source("runtime/src/entry.rs");
    let item = file
        .items
        .into_iter()
        .find_map(|item| match item {
            syn::Item::Const(item) if item.ident == "COMMANDS" => Some(item),
            _ => None,
        })
        .expect("real production command declaration");
    let mut out = literals(&item.expr);
    out.insert("panel".into());
    out
}
pub fn owned_module(name: &str) -> PathBuf {
    let file = source("runtime/src/lib.rs");
    let module = file
        .items
        .into_iter()
        .find_map(|item| match item {
            syn::Item::Mod(module) if module.ident == name => Some(module),
            _ => None,
        })
        .unwrap();
    let relative = module
        .attrs
        .iter()
        .find_map(|attr| {
            if let syn::Meta::NameValue(value) = &attr.meta {
                if value.path.is_ident("path")
                    && let syn::Expr::Lit(value) = &value.value
                    && let syn::Lit::Str(text) = &value.lit
                {
                    return Some(text.value());
                }
                None
            } else {
                None
            }
        })
        .unwrap_or_else(|| format!("{name}.rs"));
    project()
        .join("runtime/src")
        .join(relative)
        .canonicalize()
        .unwrap()
}
pub fn metadata() -> Value {
    let root = Root::new();
    let mut environment = machine();
    if let Ok(path) = std::env::var("PATH") {
        environment.insert("PATH".into(), path);
    }
    for (key, value) in [
        ("CARGO_HOME", option_env!("CARGO_HOME")),
        ("RUSTUP_HOME", option_env!("RUSTUP_HOME")),
    ] {
        if let Some(value) = value {
            environment.insert(key.into(), value.into());
        }
    }
    environment.insert("RUSTUP_AUTO_INSTALL".into(), "0".into());
    let argv = vec![
        env!("CARGO").into(),
        "metadata".into(),
        "--manifest-path".into(),
        project().join("runtime/Cargo.toml").into_os_string(),
        "--locked".into(),
        "--offline".into(),
        "--no-deps".into(),
        "--format-version".into(),
        "1".into(),
    ];
    let log = RunLog::open(&root.0, "actual-cargo-metadata", vec![]);
    let result = executor()
        .block_on(tooling_process::capture(
            &root.0,
            &argv,
            &environment,
            Duration::from_secs(30),
            Duration::from_secs(15),
            &log,
        ))
        .unwrap();
    assert_eq!(result.code, 0, "{}", result.stderr);
    serde_json::from_str(&result.stdout).unwrap()
}
pub fn launcher_requests(root: &Root) -> BTreeMap<String, BTreeSet<String>> {
    std::fs::create_dir_all(root.0.join("input with spaces")).unwrap();
    root.file(
        "collection/publish_myset.json",
        br#"{"sets":[{"fmt":"mixed","index":1}]}"#,
    );
    let mut out = BTreeMap::<String, BTreeSet<String>>::new();
    for (input, codes) in [
        (
            "a1\ninput with spaces\nmyset\nMy Emoji\n\ny\nexit\n",
            &[0, 0, 0][..],
        ),
        ("a2\ninput with spaces\noutput with spaces\nexit\n", &[][..]),
        ("a3\ny\nexit\n", &[][..]),
        ("b0\n111111111\n\nexit\n", &[][..]),
        ("b1\npack_one\n\nGENERAL_BOT_TOKEN\nexit\n", &[][..]),
        ("b2\ninput with spaces\n😀\nexit\n", &[][..]),
        ("b3\nmyset\nMy Set\n\ny\nexit\n", &[0, 0, 0][..]),
        ("b4\ny\nexit\n", &[][..]),
        ("b5\nmyset\n1\ny\nexit\n", &[][..]),
        ("c1\nexit\n", &[][..]),
        ("d2\nexit\n", &[][..]),
        ("d3\ny\nexit\n", &[3, 0][..]),
        ("d4\ny\nexit\n", &[3, 0][..]),
    ] {
        for request in root.menu(input, codes) {
            if let runtime::tooling_menu::Request::Command(args) = request {
                let flags = out.entry(args[0].clone()).or_default();
                flags.extend(args.iter().filter(|arg| arg.starts_with("--")).cloned());
            }
        }
    }
    out
}
pub fn flags(text: &str) -> BTreeSet<String> {
    text.match_indices("--")
        .filter_map(|(index, _)| {
            if text[..index]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_alphanumeric() || ch == '_' || ch == '-')
            {
                return None;
            }
            let tail = &text[index + 2..];
            if !tail.as_bytes().first().is_some_and(u8::is_ascii_lowercase) {
                return None;
            }
            let n = tail
                .bytes()
                .take_while(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || *ch == b'-')
                .count();
            Some(text[index..index + n + 2].into())
        })
        .collect()
}
pub fn sections() -> BTreeMap<String, (BTreeSet<String>, String)> {
    let text = ["docs/guide-cli.md", "docs/guide-cli-panel-coin.md"]
        .iter()
        .map(|name| std::fs::read_to_string(project().join(name)).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    let mut known = commands();
    // The original reference excludes the polling bot: it has no argparse
    // options and intentionally rejects --help rather than starting polling.
    known.remove("emoji-bot");
    let mut out = BTreeMap::new();
    let mut current = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("### 12.") {
            let (number, title) = rest.split_once(' ').unwrap();
            let id = format!("12.{number}");
            let tools: BTreeSet<String> = title
                .split('`')
                .skip(1)
                .step_by(2)
                .filter(|name| known.contains(*name))
                .map(str::to_owned)
                .collect();
            assert!(out.insert(id.clone(), (tools, title.to_owned())).is_none());
            current = Some(id);
        } else if let Some(id) = &current {
            out.get_mut(id).unwrap().1.push_str(&format!("\n{line}"));
        }
    }
    if let Some((tools, body)) = out.get_mut("12.8") {
        for word in body.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')) {
            if word.starts_with("coin-") {
                assert!(known.contains(word), "unknown documented command {word}");
                tools.insert(word.into());
            }
        }
    }
    out
}

pub fn lottie(root: &Root) -> PathBuf {
    let data = root.0.join("animation");
    std::fs::create_dir(&data).unwrap();
    use std::io::Write;
    for (key, value) in [
        (
            "good",
            json!({"v":"5.5","w":512,"h":512,"fr":60,"ip":0,"op":60,"layers":[]}),
        ),
        (
            "bomb",
            json!({"pad":"A".repeat(runtime::native_lottie::MAX_UNPACKED+1_000_000)}),
        ),
    ] {
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gzip.write_all(value.to_string().as_bytes()).unwrap();
        let path = root.file(format!("animation/{key}.tgs"), &gzip.finish().unwrap());
        let mut cat = Catalog::open(&data.join("catalog.db"), UTC, &root.0).unwrap();
        assert!(
            cat.insert(
                &json!({"content_key":key,"fmt":"animated","file_path":path}),
                UTC
            )
            .unwrap()
        );
    }
    data
}
