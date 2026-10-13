//! One pooled provider GET transport with explicit retry policy and bounded response bodies.
use reqwest::blocking::Client;
use reqwest::header::HeaderMap;
use std::io::Read;
use std::time::Duration;

pub struct Reply {
    pub status: u16,
    pub retry_after: Option<String>,
    pub bytes: Vec<u8>,
}

pub struct Retry {
    pub attempts: u32,
    pub backoff: f64,
    pub max_backoff: f64,
    pub stop_on: Vec<u16>,
}
impl Default for Retry {
    fn default() -> Self {
        Self {
            attempts: 4,
            backoff: 4.0,
            max_backoff: 20.0,
            stop_on: Vec::new(),
        }
    }
}
impl Retry {
    pub fn wait(&self, attempt: u32, header: Option<&str>) -> Result<Duration, String> {
        let fallback = (self.backoff * f64::from(attempt)).min(self.max_backoff);
        let seconds = header
            .and_then(|value| value.trim().parse::<f64>().ok())
            .map(|value| value.min(60.0))
            .filter(|value| *value != 0.0)
            .unwrap_or(fallback);
        if !seconds.is_finite() || seconds < 0.0 {
            return Err("provider retry wait must be finite and nonnegative".into());
        }
        Ok(Duration::from_secs_f64(seconds))
    }
}

pub fn get_with<F, W>(policy: &Retry, mut request: F, mut wait: W) -> Result<Option<Reply>, String>
where
    F: FnMut() -> Result<Reply, String>,
    W: FnMut(Duration) -> Result<(), String>,
{
    if policy.attempts > 20
        || !policy.backoff.is_finite()
        || policy.backoff < 0.0
        || !policy.max_backoff.is_finite()
        || policy.max_backoff < 0.0
    {
        return Err("invalid provider retry policy".into());
    }
    for attempt in 1..=policy.attempts {
        let response = request();
        if let Ok(reply) = &response
            && (reply.status < 400 || policy.stop_on.contains(&reply.status))
        {
            return Ok(response.ok());
        }
        if attempt == policy.attempts {
            break;
        }
        let header = response
            .as_ref()
            .ok()
            .and_then(|reply| reply.retry_after.as_deref());
        wait(policy.wait(attempt, header)?)?;
    }
    Ok(None)
}

pub struct ProviderHttp {
    client: Client,
    timeout: Duration,
}
impl ProviderHttp {
    pub fn new(timeout: Duration) -> Result<Self, String> {
        if timeout.is_zero() || timeout > Duration::from_secs(300) {
            return Err("invalid provider timeout".into());
        }
        let mut builder = Client::builder()
            .timeout(timeout)
            .connect_timeout(timeout.min(Duration::from_secs(15)))
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never());
        if std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1") {
            builder = builder.no_proxy();
        }
        Ok(Self {
            client: builder
                .build()
                .map_err(|_| "cannot initialize provider HTTP client")?,
            timeout,
        })
    }

    pub fn get(
        &self,
        url: &str,
        headers: &HeaderMap,
        policy: &Retry,
    ) -> Result<Option<Reply>, String> {
        let url = reqwest::Url::parse(url).map_err(|_| "invalid provider URL")?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err("unsupported provider URL".into());
        }
        if std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1")
            && !matches!(url.host_str(), Some("127.0.0.1" | "::1" | "[::1]"))
        {
            return Err("native tests may connect only to numeric loopback hosts".into());
        }
        get_with(
            policy,
            || {
                let mut response = self
                    .client
                    .get(url.clone())
                    .headers(headers.clone())
                    .timeout(self.timeout)
                    .send()
                    .map_err(|_| "provider transport unavailable".to_owned())?;
                let status = response.status().as_u16();
                let retry_after = response
                    .headers()
                    .get("Retry-After")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned);
                let mut bytes = Vec::new();
                (&mut response)
                    .take(8 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| "cannot read provider response".to_owned())?;
                if bytes.len() > 8 * 1024 * 1024 {
                    return Err("provider response exceeds8MiB".into());
                }
                Ok(Reply {
                    status,
                    retry_after,
                    bytes,
                })
            },
            |duration| {
                std::thread::sleep(duration);
                Ok(())
            },
        )
    }
}
