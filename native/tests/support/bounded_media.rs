//! Native consumer fixtures share one deadline across calls and relocated codec workspaces.
use _native::{media_adapter::MediaCodec, media_native_adapter::NativeMediaAdapter};
use serde_json::Value;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub struct BoundedCodec {
    pub native: NativeMediaAdapter,
    pub deadline: Instant,
}
impl MediaCodec for BoundedCodec {
    fn with_scratch(&self, scratch: PathBuf) -> Self {
        Self {
            native: self.native.with_scratch(scratch),
            deadline: self.deadline,
        }
    }

    async fn call(&self, request: Value) -> Result<Value, String> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("native consumer fixture exceeded60s; evidence retained".into());
        }
        self.native
            .call_with_budget(
                request,
                remaining.min(Duration::from_secs(20)),
                remaining.min(Duration::from_secs(10)),
            )
            .await
    }
}
