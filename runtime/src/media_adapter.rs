//! Native codec interface shared by collection, publication and preview policies.
use serde_json::Value;
use std::path::{Path, PathBuf};
pub type MediaAdapter = crate::media_native_adapter::NativeMediaAdapter;

pub trait MediaCodec: Sync {
    fn with_scratch(&self, scratch: PathBuf) -> Self;

    fn call(&self, request: Value) -> impl Future<Output = Result<Value, String>> + Send;

    fn publication_usable<'a>(
        &'a self,
        source: &'a Path,
        format: &'a str,
    ) -> impl Future<Output = Result<bool, String>> + Send + 'a {
        async move {
            self.call(serde_json::json!({"operation":"publication_usable","source":source,"format":format}))
                .await?
                .as_bool()
                .ok_or_else(|| "invalid publication media admission result".into())
        }
    }

    fn compare<'a>(
        &'a self,
        a: &'a Path,
        b: &'a Path,
        format: &'a str,
    ) -> impl Future<Output = Result<Option<bool>, String>> + Send + 'a {
        async move {
            let value = self
                .call(
                    serde_json::json!({"operation":"compare","source":a,"other":b,"format":format}),
                )
                .await?;
            if value.is_null() {
                Ok(None)
            } else {
                value
                    .as_bool()
                    .map(Some)
                    .ok_or_else(|| "invalid media comparison result".into())
            }
        }
    }
}
