use crate::baseline_catalog_support as catalog_fixture;
#[allow(unused_imports)]
pub use catalog_fixture::{Scratch, UTC};
use numera_runtime as production;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
pub fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
pub fn png(path: &Path, color: [u8; 4]) -> PathBuf {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbaImage::from_pixel(100, 100, image::Rgba(color))
        .save(path)
        .unwrap();
    path.into()
}
#[derive(Clone, Default)]
pub struct Images;
impl production::media_adapter::MediaCodec for Images {
    fn with_scratch(&self, _: PathBuf) -> Self {
        Self
    }
    async fn call(&self, request: Value) -> Result<Value, String> {
        let source = PathBuf::from(request["source"].as_str().ok_or("missing source")?);
        match request["operation"].as_str().unwrap_or("") {
            "convert" => {
                tokio::task::spawn_blocking(move || production::codec_worker::execute(&request))
                    .await
                    .map_err(|e| e.to_string())?
            }
            "prepare" => production::codec_image::prepare(&source, &request, &mut Vec::new())
                .map_err(|e| e.message),
            "collision" => production::codec_format::collision_request(&source, &request["key"])
                .map_err(|e| e.message),
            "fingerprint" => {
                production::codec_image::fingerprint_typed(&source).map_err(|e| e.message)
            }
            "same_image" | "compare" => {
                let other = PathBuf::from(request["other"].as_str().ok_or("missing other")?);
                Ok(json!(production::codec_image::same_image(&source, &other)))
            }
            "usable" | "publication_usable" => {
                production::codec_image::metadata_operation(&source, "usable")
                    .map_err(|e| e.message)
            }
            other => Err(format!("unexpected recovery image operation {other}")),
        }
    }
}
pub fn files(root: &Path) -> Vec<PathBuf> {
    let mut v = std::fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    v.sort();
    v
}
pub mod order;
#[allow(unused_imports)]
pub(crate) use crate::baseline_publication_support::server;
#[allow(unused_imports)]
pub use crate::baseline_publication_support::{Bot, Model, sticker};
pub fn config(root: &Path, extra: &[(&str, &str)]) -> production::config::Config {
    let mut env = std::collections::BTreeMap::from([
        ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
        ("BRAND_LOGO_BOTS".into(), "".into()),
        ("FAKE_TOKEN".into(), "test-only-token".into()),
        ("PACK_OWNER_USER_ID".into(), "1".into()),
    ]);
    for (k, v) in extra {
        env.insert(k.to_string(), v.to_string());
    }
    production::config::Config::load_with_environment(root.into(), &env).unwrap()
}
#[allow(unused_imports)]
pub use crate::baseline_publication_support::read_json as read;
