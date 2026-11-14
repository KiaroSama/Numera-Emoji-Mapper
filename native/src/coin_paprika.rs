//! CoinPaprika search uses the shared bounded transport and source confidence/unknown rules.
use crate::coin_http::{ProviderHttp, Retry};
use crate::coin_provider_resolve::Search;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde_json::Value;
use std::time::Duration;

pub enum Response {
    Payload(Value),
    Quota,
    Transient,
}

pub fn http_json(client: &ProviderHttp, url: &str) -> Result<Response, String> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Mozilla/5.0 (logo-fetcher; local)"),
    );
    let policy = Retry {
        stop_on: vec![402],
        ..Retry::default()
    };
    let Some(reply) = client.get(url, &headers, &policy)? else {
        return Ok(Response::Transient);
    };
    if reply.status == 402 {
        return Ok(Response::Quota);
    }
    serde_json::from_slice(&reply.bytes)
        .map(Response::Payload)
        .map_err(|_| "invalid CoinPaprika JSON".into())
}

pub fn http_bytes(client: &ProviderHttp, url: &str) -> Result<Option<Vec<u8>>, String> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Mozilla/5.0 (logo-fetcher; local)"),
    );
    let policy = Retry {
        attempts: 3,
        backoff: 2.0,
        max_backoff: 6.0,
        stop_on: Vec::new(),
    };
    Ok(client.get(url, &headers, &policy)?.map(|reply| reply.bytes))
}

fn candidates(response: &Response) -> Result<&[Value], String> {
    match response {
        Response::Transient => Ok(&[]),
        Response::Payload(value) if value.is_null() => Ok(&[]),
        Response::Payload(value) => value.get("currencies").map_or(Ok(&[]), |list| {
            list.as_array()
                .map(Vec::as_slice)
                .ok_or_else(|| "CoinPaprika currencies must be a list".into())
        }),
        Response::Quota => Err("quota response cannot contain candidates".into()),
    }
}

pub fn search_with<F, W>(
    name: &str,
    ticker: &str,
    mut get: F,
    mut wait: W,
) -> Result<Search, String>
where
    F: FnMut(&str) -> Result<Response, String>,
    W: FnMut(Duration) -> Result<(), String>,
{
    let mut query = String::new();
    let mut rest = name;
    while let Some(start) = rest.find('(') {
        query.push_str(&rest[..start]);
        let suffix = &rest[start + 1..];
        if let Some(end) = suffix.split('\n').next().unwrap_or("").find(')') {
            rest = &suffix[end + 1..];
        } else {
            query.push('(');
            rest = suffix;
        }
    }
    query.push_str(rest);
    let first = get(query.trim())?;
    if matches!(first, Response::Quota) {
        return Ok(Search::Quota);
    }
    wait(Duration::from_millis(2500))?;
    let mut transient = matches!(first, Response::Transient);
    let first_candidates = candidates(&first)?;
    let classified = crate::coin_provider_policy::classify(first_candidates, name, ticker)?;
    if let Some(id) = classified[0].as_str().filter(|id| !id.is_empty()) {
        return Ok(Search::Match {
            id: Some(id.into()),
            confidence: classified[1]
                .as_str()
                .ok_or("invalid candidate confidence")?
                .into(),
        });
    }
    let confidence = classified[1]
        .as_str()
        .ok_or("invalid candidate confidence")?;
    if first_candidates.is_empty()
        || confidence.starts_with("no-confident")
        || confidence == "no-results"
    {
        let second = get(ticker)?;
        if matches!(second, Response::Quota) {
            return Ok(Search::Quota);
        }
        wait(Duration::from_millis(2500))?;
        transient |= matches!(second, Response::Transient);
        let second_candidates = candidates(&second)?;
        let classified_second =
            crate::coin_provider_policy::classify(second_candidates, name, ticker)?;
        if let Some(id) = classified_second[0].as_str().filter(|id| !id.is_empty()) {
            return Ok(Search::Match {
                id: Some(id.into()),
                confidence: classified_second[1]
                    .as_str()
                    .ok_or("invalid candidate confidence")?
                    .into(),
            });
        }
        if transient {
            return Ok(Search::Transient);
        }
        return Ok(Search::Match {
            id: None,
            confidence: if second_candidates.is_empty() {
                confidence.into()
            } else {
                classified_second[1]
                    .as_str()
                    .ok_or("invalid candidate confidence")?
                    .into()
            },
        });
    }
    Ok(Search::Match {
        id: None,
        confidence: confidence.into(),
    })
}

pub fn search(client: &ProviderHttp, name: &str, ticker: &str) -> Result<Search, String> {
    search_with(
        name,
        ticker,
        |query| {
            let mut url = reqwest::Url::parse("https://api.coinpaprika.com/v1/search")
                .map_err(|_| "invalid provider search URL")?;
            url.query_pairs_mut()
                .append_pair("c", "currencies")
                .append_pair("limit", "10")
                .append_pair("q", query);
            http_json(client, url.as_str())
        },
        |duration| {
            std::thread::sleep(duration);
            Ok(())
        },
    )
}
