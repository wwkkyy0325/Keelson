//! Web tools: the connected model's own provider first, a separately
//! configured backend only when that fails, and a plain HTTP fetch reduced
//! to text as the last resort.
//!
//! DeepSeek serves search and fetch at `{base}/v1/tools/{search,fetch}` on
//! the chat key. Volcengine Ark serves a hosted `web_search` tool on its
//! Responses API, on the model's key. Any other provider gets only the local
//! fetch, which needs no key.

use std::collections::HashSet;
use std::net::IpAddr;
use std::time::Duration;

use async_trait::async_trait;
use keel_core::{
    ArgValue, ErrorCode, ParamKind, ParamSpec, PermissionRequest, RiskLevel, Tool, ToolArgs,
    ToolCtx, ToolError, ToolResult, ToolSchema,
};
use reqwest::{Client, Url};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// The provider fetches the page on its side, so a slow origin is out of our
/// control; the bound is wider than a direct download's.
const PROVIDER_FETCH_TIMEOUT: Duration = Duration::from_secs(60);
/// A hosted search is one model call with its own reasoning; it gets a wider
/// bound than a plain endpoint query.
const HOSTED_SEARCH_TIMEOUT: Duration = Duration::from_secs(90);
/// How many search terms the hosted tool may expand a query into.
const HOSTED_MAX_KEYWORD: i64 = 3;
/// The most of a page the local fetch keeps.
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
/// Characters kept from a fetched page.
const MAX_TEXT_CHARS: usize = 60_000;
/// Characters kept from a whole search result list.
const MAX_SEARCH_CHARS: usize = 8_000;
const MAX_SUMMARY_CHARS: usize = 800;
const MAX_RESULTS: i64 = 20;
const DEFAULT_RESULTS: i64 = 8;
/// The provider whose plain search endpoint a saved key can reach.
const DEEPSEEK_BASE: &str = "https://api.deepseek.com";

/// Where web tools get their data, in the order they are tried: the
/// connected model's own provider first, a separately configured backend
/// only when that fails.
#[derive(Clone)]
pub enum WebBackend {
    /// A hosted search tool on the provider's Responses API, run on the
    /// model's own key (Volcengine Ark serves `web_search` this way).
    Hosted {
        endpoint: String,
        model: String,
        key: String,
    },
    /// The provider's plain search and fetch endpoints (DeepSeek).
    Provider { base: String, key: String },
    /// A self-hosted SearXNG instance. It searches; fetch stays local.
    Searxng { url: String },
}

impl WebBackend {
    /// The search backends in the order they are tried. Empty means the
    /// model has no web search at all.
    pub fn chain(
        base_url: Option<&str>,
        model: &str,
        api_key: &str,
        search_key: Option<&str>,
        search_url: Option<&str>,
    ) -> Vec<Self> {
        let mut backends = Vec::new();
        if let Some(backend) = connected(base_url, model, api_key) {
            backends.push(backend);
        }
        if let Some(key) = search_key.map(str::trim).filter(|key| !key.is_empty()) {
            // The same service on the same key would repeat the request that
            // just failed, so it is not tried twice.
            if !already_searches_on(&backends, key) {
                backends.push(Self::Provider {
                    base: DEEPSEEK_BASE.to_owned(),
                    key: key.to_owned(),
                });
            }
        }
        if let Some(url) = searxng_url(search_url) {
            backends.push(Self::Searxng { url });
        }
        backends
    }

    /// Only a provider endpoint serves fetch; a hosted search and SearXNG
    /// search do not.
    pub fn serves_fetch(&self) -> bool {
        matches!(self, Self::Provider { .. })
    }

    fn label(&self) -> String {
        match self {
            Self::Hosted { model, .. } => format!("the model's own search ({model})"),
            Self::Provider { .. } => "the provider search endpoint".to_owned(),
            Self::Searxng { .. } => "the configured search service".to_owned(),
        }
    }
}

/// The connected model's own search facility, when its provider has one.
fn connected(base_url: Option<&str>, model: &str, api_key: &str) -> Option<WebBackend> {
    let base = base_url?.trim().trim_end_matches('/');
    let key = api_key.trim();
    if base.is_empty() || key.is_empty() {
        return None;
    }
    let url = Url::parse(base).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    if is_ark_host(&host) {
        let model = model.trim();
        if model.is_empty() {
            return None;
        }
        // The Responses API keeps its versioned path whatever the chat base
        // is, the same way DeepSeek's tools live at the API root.
        return Some(WebBackend::Hosted {
            endpoint: format!("{}/api/v3/responses", url.origin().ascii_serialization()),
            model: model.to_owned(),
            key: key.to_owned(),
        });
    }
    if host == "api.deepseek.com" || host == "api.deepseek.cn" {
        return Some(WebBackend::Provider {
            base: base.to_owned(),
            key: key.to_owned(),
        });
    }
    None
}

/// A saved key is skipped when the connected model already searches through
/// the same service on the same key.
fn already_searches_on(backends: &[WebBackend], key: &str) -> bool {
    backends.iter().any(|backend| match backend {
        WebBackend::Provider { base, key: known } => {
            tool_origin(base) == DEEPSEEK_BASE && known.as_str() == key
        }
        _ => false,
    })
}

/// Volcengine Ark serves a hosted `web_search` tool on its Responses API.
fn is_ark_host(host: &str) -> bool {
    host.starts_with("ark.") && host.ends_with(".volces.com")
}

fn searxng_url(search_url: Option<&str>) -> Option<String> {
    let url = search_url?.trim();
    if url.is_empty() {
        return None;
    }
    let parsed = Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return None;
    }
    Some(url.trim_end_matches('/').to_owned())
}

fn provider_tools_url(base: &str, leaf: &str) -> String {
    format!("{}/v1/tools/{leaf}", tool_origin(base))
}

/// The chat base may carry a compatibility path (`/anthropic`, `/v1`); the
/// provider's tools and search are served from the API root.
fn tool_origin(base: &str) -> String {
    Url::parse(base)
        .map(|url| url.origin().ascii_serialization())
        .unwrap_or_else(|_| base.trim_end_matches('/').to_owned())
}

fn searxng_search_url(url: &str, query: &str) -> Option<Url> {
    let mut endpoint = Url::parse(&format!("{url}/search")).ok()?;
    endpoint
        .query_pairs_mut()
        .append_pair("q", query)
        .append_pair("format", "json");
    Some(endpoint)
}

fn client() -> Client {
    Client::builder()
        .user_agent("keelson-web/0.1")
        .build()
        .unwrap_or_else(|_| Client::new())
}

/// reqwest's message names the request but leaves the reason ("operation
/// timed out", "connection refused") in the source chain. Append it, so a
/// model can tell a slow origin from a bad URL.
fn error_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut message = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        let text = cause.to_string();
        if !text.is_empty() && !message.contains(&text) {
            message.push_str(": ");
            message.push_str(&text);
        }
        source = std::error::Error::source(cause);
    }
    message
}

/// `web_search`: the connected model's own search first, then the configured
/// backends. Results are titles, URLs and summaries; `web_fetch` reads one.
pub struct WebSearchTool {
    backends: Vec<WebBackend>,
    client: Client,
}

impl WebSearchTool {
    pub fn new(backends: Vec<WebBackend>) -> Self {
        Self {
            backends,
            client: client(),
        }
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "web_search".to_owned(),
            description: "Search the web. The connected model's provider is tried first, then \
                          the configured search service. Returns up to `count` results with a \
                          title, URL and summary; a provider endpoint also understands `site:`, \
                          `-site:`, `\"exact phrase\"`, `lang:`, `after:` and `before:`. Use \
                          web_fetch to read a result in full."
                .to_owned(),
            risk: RiskLevel::ReadOnly,
            params: vec![
                ParamSpec {
                    name: "query".to_owned(),
                    kind: ParamKind::String,
                    required: true,
                },
                ParamSpec {
                    name: "count".to_owned(),
                    kind: ParamKind::Integer,
                    required: false,
                },
            ],
        }
    }

    fn permission_requests(&self, _args: &ToolArgs) -> Result<Vec<PermissionRequest>, ToolError> {
        Ok(Vec::new())
    }

    async fn call(&self, args: ToolArgs, ctx: &ToolCtx) -> ToolResult {
        let query = match args.require_str("query") {
            Ok(query) => query.trim().to_owned(),
            Err(error) => return ToolResult::failure(ctx.call_id, "web_search", error),
        };
        if query.is_empty() {
            return ToolResult::failure(
                ctx.call_id,
                "web_search",
                ToolError::invalid_arguments("`query` must not be empty"),
            );
        }
        if query.chars().count() > 128 {
            return ToolResult::failure(
                ctx.call_id,
                "web_search",
                ToolError::invalid_arguments("`query` is longer than 128 characters"),
            );
        }
        let count = match args.get("count") {
            Some(ArgValue::Integer(value)) => (*value).clamp(1, MAX_RESULTS),
            Some(_) => {
                return ToolResult::failure(
                    ctx.call_id,
                    "web_search",
                    ToolError::invalid_arguments("`count` must be an integer"),
                )
            }
            None => DEFAULT_RESULTS,
        };
        // A cancelled turn drops the request instead of waiting for the
        // timeout; the provider bills only a successful request.
        let results = match tokio::select! {
            _ = ctx.cancel.cancelled() => Err(ToolError::cancelled()),
            result = self.search(&query, count) => result,
        } {
            Ok(results) => results,
            Err(error) => return ToolResult::failure(ctx.call_id, "web_search", error),
        };
        if results.is_empty() {
            return ToolResult::success(
                ctx.call_id,
                "web_search",
                format!("no results for `{query}`"),
            );
        }
        ToolResult::success(ctx.call_id, "web_search", render_results(&results))
    }
}

impl WebSearchTool {
    /// Each backend is tried in turn; the first one that answers with
    /// results wins. An empty answer moves on, so a provider whose response
    /// shape the host cannot read still leaves the configured search to
    /// answer; only when every backend fails is the call an error.
    async fn search(&self, query: &str, count: i64) -> Result<Vec<serde_json::Value>, ToolError> {
        let mut answered = false;
        let mut failures = Vec::new();
        for backend in &self.backends {
            match search_with(&self.client, backend, query, count).await {
                Ok(results) if results.is_empty() => answered = true,
                Ok(results) => return Ok(results),
                Err(error) => failures.push(format!("{}: {}", backend.label(), error.message)),
            }
        }
        if answered {
            return Ok(Vec::new());
        }
        Err(ToolError::io(format!(
            "every search backend failed ({})",
            failures.join("; ")
        )))
    }
}

async fn search_with(
    client: &Client,
    backend: &WebBackend,
    query: &str,
    count: i64,
) -> Result<Vec<serde_json::Value>, ToolError> {
    match backend {
        WebBackend::Hosted {
            endpoint,
            model,
            key,
        } => hosted_search(client, endpoint, model, key, query).await,
        WebBackend::Provider { base, key } => {
            provider_search(client, base, key, query, count).await
        }
        WebBackend::Searxng { url } => searxng_search(client, url, query, count).await,
    }
}

/// Runs the provider's own web search tool. The answer's citations are the
/// results; when the provider returns no citations the answer itself is the
/// one result, so the model still reads what the search found.
async fn hosted_search(
    client: &Client,
    endpoint: &str,
    model: &str,
    key: &str,
    query: &str,
) -> Result<Vec<serde_json::Value>, ToolError> {
    let body = serde_json::json!({
        "model": model,
        "tools": [{ "type": "web_search", "max_keyword": HOSTED_MAX_KEYWORD }],
        "input": [{
            "role": "user",
            "content": [{
                "type": "input_text",
                "text": format!(
                    "Search the web for: {query}\nAnswer briefly and cite the sources you used."
                ),
            }],
        }],
    });
    let response = client
        .post(endpoint)
        .bearer_auth(key)
        .json(&body)
        .timeout(HOSTED_SEARCH_TIMEOUT)
        .send()
        .await
        .map_err(|error| {
            ToolError::io(format!("search request failed: {}", error_chain(&error)))
        })?;
    let status = response.status();
    let value: serde_json::Value = response.json().await.map_err(|error| {
        ToolError::new(
            ErrorCode::InvalidResponse,
            format!("search returned a non-JSON body: {}", error_chain(&error)),
            true,
        )
    })?;
    if !status.is_success() {
        let message =
            error_message(&value).unwrap_or_else(|| format!("search returned HTTP {status}"));
        let code = if status.as_u16() == 400 {
            ErrorCode::InvalidArguments
        } else {
            ErrorCode::Io
        };
        return Err(ToolError::new(code, message, code == ErrorCode::Io));
    }
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    collect_sources(&value, &mut results, &mut seen);
    if results.is_empty() {
        let answer = response_text(&value);
        if !answer.trim().is_empty() {
            results.push(serde_json::json!({
                "title": "web search answer",
                "url": "",
                "summary": clip(&answer, MAX_SUMMARY_CHARS),
            }));
        }
    }
    Ok(results)
}

/// Citations reach the host as `url_citation` annotations on the answer or
/// as `sources` on a search call; both shapes are read.
fn collect_sources(
    value: &serde_json::Value,
    out: &mut Vec<serde_json::Value>,
    seen: &mut HashSet<String>,
) {
    match value {
        serde_json::Value::Object(map) => {
            if map.get("type").and_then(|kind| kind.as_str()) == Some("url_citation") {
                push_source(
                    map.get("url").and_then(|url| url.as_str()),
                    map.get("title").and_then(|title| title.as_str()),
                    out,
                    seen,
                );
            }
            if let Some(sources) = map.get("sources").and_then(|sources| sources.as_array()) {
                for source in sources {
                    match source {
                        serde_json::Value::Object(source) => push_source(
                            source.get("url").and_then(|url| url.as_str()),
                            source.get("title").and_then(|title| title.as_str()),
                            out,
                            seen,
                        ),
                        serde_json::Value::String(url) => {
                            push_source(Some(url.as_str()), None, out, seen)
                        }
                        _ => {}
                    }
                }
            }
            for child in map.values() {
                collect_sources(child, out, seen);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_sources(item, out, seen);
            }
        }
        _ => {}
    }
}

fn push_source(
    url: Option<&str>,
    title: Option<&str>,
    out: &mut Vec<serde_json::Value>,
    seen: &mut HashSet<String>,
) {
    let Some(url) = url else { return };
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return;
    }
    if !seen.insert(url.to_owned()) {
        return;
    }
    let title = title
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or(url);
    out.push(serde_json::json!({ "title": title, "url": url, "summary": "" }));
}

fn response_text(value: &serde_json::Value) -> String {
    let mut out = String::new();
    let Some(output) = value.get("output").and_then(|output| output.as_array()) else {
        return out;
    };
    for item in output {
        let Some(contents) = item.get("content").and_then(|content| content.as_array()) else {
            continue;
        };
        for content in contents {
            if content.get("type").and_then(|kind| kind.as_str()) != Some("output_text") {
                continue;
            }
            let Some(text) = content.get("text").and_then(|text| text.as_str()) else {
                continue;
            };
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(text);
        }
    }
    out
}

async fn provider_search(
    client: &Client,
    base: &str,
    key: &str,
    query: &str,
    count: i64,
) -> Result<Vec<serde_json::Value>, ToolError> {
    let body = serde_json::json!({ "query": query, "count": count });
    let response = client
        .post(provider_tools_url(base, "search"))
        .bearer_auth(key)
        .json(&body)
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| {
            ToolError::io(format!("search request failed: {}", error_chain(&error)))
        })?;
    let status = response.status();
    let value: serde_json::Value = response.json().await.map_err(|error| {
        ToolError::new(
            ErrorCode::InvalidResponse,
            format!("search returned a non-JSON body: {}", error_chain(&error)),
            true,
        )
    })?;
    if !status.is_success() {
        let message =
            error_message(&value).unwrap_or_else(|| format!("search returned HTTP {status}"));
        let code = if status.as_u16() == 400 {
            ErrorCode::InvalidArguments
        } else {
            ErrorCode::Io
        };
        return Err(ToolError::new(code, message, code == ErrorCode::Io));
    }
    Ok(value
        .get("results")
        .and_then(|results| results.as_array())
        .cloned()
        .unwrap_or_default())
}

async fn searxng_search(
    client: &Client,
    base: &str,
    query: &str,
    count: i64,
) -> Result<Vec<serde_json::Value>, ToolError> {
    let endpoint = searxng_search_url(base, query).ok_or_else(|| {
        ToolError::invalid_arguments("the configured search URL is not a valid address")
    })?;
    let response = client
        .get(endpoint)
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| {
            ToolError::io(format!("search request failed: {}", error_chain(&error)))
        })?;
    let status = response.status();
    let text = response.text().await.map_err(|error| {
        ToolError::io(format!(
            "reading the search response failed: {}",
            error_chain(&error)
        ))
    })?;
    if !status.is_success() {
        return Err(ToolError::new(
            ErrorCode::Io,
            format!("the search service returned HTTP {status}"),
            true,
        ));
    }
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|_| {
        ToolError::new(
            ErrorCode::InvalidResponse,
            "the search service returned non-JSON; enable the json format in its settings",
            true,
        )
    })?;
    let mut results = value
        .get("results")
        .and_then(|results| results.as_array())
        .cloned()
        .unwrap_or_default();
    results.truncate(usize::try_from(count).unwrap_or(usize::MAX));
    Ok(results)
}

/// `web_fetch`: the provider's fetch endpoint when there is one (it returns
/// Markdown), otherwise a local download reduced to text.
pub struct WebFetchTool {
    backend: Option<WebBackend>,
    client: Client,
}

impl WebFetchTool {
    pub fn new(backend: Option<WebBackend>) -> Self {
        Self {
            backend,
            client: client(),
        }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "web_fetch".to_owned(),
            description: "Fetch one web page as text. Use it after web_search or when the user \
                          gives a URL. The provider's fetch endpoint returns Markdown; without \
                          one the page is downloaded and reduced to text."
                .to_owned(),
            risk: RiskLevel::ReadOnly,
            params: vec![ParamSpec {
                name: "url".to_owned(),
                kind: ParamKind::String,
                required: true,
            }],
        }
    }

    fn permission_requests(&self, _args: &ToolArgs) -> Result<Vec<PermissionRequest>, ToolError> {
        Ok(Vec::new())
    }

    async fn call(&self, args: ToolArgs, ctx: &ToolCtx) -> ToolResult {
        let input = match args.require_str("url") {
            Ok(input) => input.trim().to_owned(),
            Err(error) => return ToolResult::failure(ctx.call_id, "web_fetch", error),
        };
        let url = match validate_url(&input) {
            Ok(url) => url,
            Err(message) => {
                return ToolResult::failure(
                    ctx.call_id,
                    "web_fetch",
                    ToolError::invalid_arguments(message),
                )
            }
        };
        let fetch = async {
            match &self.backend {
                Some(WebBackend::Provider { base, key }) => {
                    provider_fetch(&self.client, base, key, &url).await
                }
                _ => local_fetch(&self.client, &url).await,
            }
        };
        // The provider may spend half a minute on a slow origin; a cancelled
        // turn must not wait for it.
        let result = tokio::select! {
            _ = ctx.cancel.cancelled() => Err(ToolError::cancelled()),
            result = fetch => result,
        };
        match result {
            Ok(text) => ToolResult::success(ctx.call_id, "web_fetch", text),
            Err(error) => ToolResult::failure(ctx.call_id, "web_fetch", error),
        }
    }
}

async fn provider_fetch(
    client: &Client,
    base: &str,
    key: &str,
    url: &Url,
) -> Result<String, ToolError> {
    let body = serde_json::json!({ "url": url.as_str() });
    let response = client
        .post(provider_tools_url(base, "fetch"))
        .bearer_auth(key)
        .json(&body)
        .timeout(PROVIDER_FETCH_TIMEOUT)
        .send()
        .await
        .map_err(|error| ToolError::io(format!("fetch request failed: {}", error_chain(&error))))?;
    let status = response.status();
    let value: serde_json::Value = response.json().await.map_err(|error| {
        ToolError::new(
            ErrorCode::InvalidResponse,
            format!("fetch returned a non-JSON body: {}", error_chain(&error)),
            true,
        )
    })?;
    if !status.is_success() {
        let message =
            error_message(&value).unwrap_or_else(|| format!("fetch returned HTTP {status}"));
        let code = match status.as_u16() {
            400 => ErrorCode::InvalidArguments,
            404 => ErrorCode::NotFound,
            _ => ErrorCode::Io,
        };
        return Err(ToolError::new(code, message, code == ErrorCode::Io));
    }
    let title = value
        .get("title")
        .and_then(|title| title.as_str())
        .unwrap_or("");
    let content = value
        .get("content")
        .and_then(|content| content.as_str())
        .unwrap_or("");
    if content.trim().is_empty() {
        return Err(ToolError::not_found("the page had no readable content"));
    }
    Ok(render_page(title, content))
}

async fn local_fetch(client: &Client, url: &Url) -> Result<String, ToolError> {
    if let Some(host) = url.host_str() {
        if is_private_host(host) {
            return Err(ToolError::invalid_arguments(
                "private and loopback hosts are not fetched",
            ));
        }
    }
    let response = client
        .get(url.clone())
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| ToolError::io(format!("fetch failed: {}", error_chain(&error))))?;
    let status = response.status();
    if !status.is_success() {
        let code = if status.as_u16() == 404 {
            ErrorCode::NotFound
        } else {
            ErrorCode::Io
        };
        return Err(ToolError::new(
            code,
            format!("the page returned HTTP {status}"),
            code == ErrorCode::Io,
        ));
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let text_type = content_type.is_empty()
        || content_type.starts_with("text/")
        || content_type.contains("json")
        || content_type.contains("xml");
    if !text_type {
        return Err(ToolError::invalid_arguments(format!(
            "unsupported content type `{content_type}`"
        )));
    }
    let mut response = response;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        ToolError::io(format!("reading the page failed: {}", error_chain(&error)))
    })? {
        let room = MAX_BODY_BYTES.saturating_sub(body.len());
        if room == 0 {
            break;
        }
        let take = chunk.len().min(room);
        body.extend_from_slice(&chunk[..take]);
        if take < chunk.len() {
            break;
        }
    }
    let body = String::from_utf8_lossy(&body);
    let text = if content_type.contains("html") {
        html_to_text(&body)
    } else {
        body.to_string()
    };
    if text.trim().is_empty() {
        return Err(ToolError::not_found("the page had no readable text"));
    }
    Ok(clip(&text, MAX_TEXT_CHARS))
}

fn validate_url(input: &str) -> Result<Url, String> {
    if input.chars().count() > 8192 {
        return Err("the URL is longer than 8192 characters".to_owned());
    }
    let url = Url::parse(input).map_err(|error| format!("`{input}` is not a URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("only http and https URLs are supported".to_owned());
    }
    if url.host_str().is_none() {
        return Err("the URL needs a host".to_owned());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("a URL with credentials is not allowed".to_owned());
    }
    Ok(url)
}

fn is_private_host(host: &str) -> bool {
    let host = host.trim_matches(['[', ']']).to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(address)) => {
            address.is_loopback()
                || address.is_private()
                || address.is_link_local()
                || address.is_unspecified()
                || address.is_broadcast()
        }
        Ok(IpAddr::V6(address)) => {
            let first = address.segments()[0];
            address.is_loopback()
                || address.is_unspecified()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
        }
        Err(_) => false,
    }
}

fn render_results(results: &[serde_json::Value]) -> String {
    let mut out = String::new();
    for (index, result) in results.iter().enumerate() {
        let title = field(result, "title");
        let url = field(result, "url");
        let summary = {
            let long = field(result, "summary");
            if long.is_empty() {
                field(result, "snippet")
            } else {
                long
            }
        };
        let date = field(result, "date");
        out.push_str(&format!("{}. {title}\n", index + 1));
        if !url.is_empty() {
            out.push_str(&format!("   {url}\n"));
        }
        if !summary.is_empty() {
            out.push_str("   ");
            out.push_str(&clip(&summary, MAX_SUMMARY_CHARS));
            out.push('\n');
        }
        if !date.is_empty() {
            out.push_str(&format!("   published: {date}\n"));
        }
        if out.chars().count() >= MAX_SEARCH_CHARS {
            out.push_str("   (more results omitted)\n");
            break;
        }
    }
    out
}

fn render_page(title: &str, content: &str) -> String {
    let body = clip(content, MAX_TEXT_CHARS);
    if title.trim().is_empty() {
        body
    } else {
        format!("# {}\n\n{body}", title.trim())
    }
}

fn field(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .trim()
        .to_owned()
}

fn error_message(value: &serde_json::Value) -> Option<String> {
    value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(|message| message.as_str())
        .map(str::to_owned)
}

fn clip(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut short: String = text.chars().take(max_chars).collect();
    short.push('…');
    short
}

const BLOCK_TAGS: &[&str] = &[
    "p",
    "div",
    "br",
    "li",
    "ul",
    "ol",
    "tr",
    "td",
    "th",
    "table",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "section",
    "article",
    "header",
    "footer",
    "pre",
    "blockquote",
    "hr",
    "title",
];

/// A small HTML reduction: drop scripts, styles and comments, turn block tags
/// into line breaks, decode common entities and collapse blank lines.
fn html_to_text(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len() / 2);
    let mut index = 0usize;
    while index < lower.len() {
        if lower.as_bytes()[index] == b'<' {
            let rest = &lower[index..];
            if rest.starts_with("<!--") {
                index = rest.find("-->").map_or(lower.len(), |end| index + end + 3);
                continue;
            }
            if rest.starts_with("<script") || rest.starts_with("<style") {
                let closing = if rest.starts_with("<script") {
                    "</script"
                } else {
                    "</style"
                };
                match rest.find(closing) {
                    Some(end) => {
                        let after = index + end + closing.len();
                        index = lower[after..]
                            .find('>')
                            .map_or(lower.len(), |gt| after + gt + 1);
                    }
                    None => index = lower.len(),
                }
                continue;
            }
            match rest.find('>') {
                Some(end) => {
                    let tag = rest[1..end].trim_start_matches('/');
                    let name = tag
                        .split(|character: char| character.is_whitespace() || character == '/')
                        .next()
                        .unwrap_or("");
                    if BLOCK_TAGS.contains(&name) {
                        out.push('\n');
                    }
                    index += end + 1;
                }
                None => index += 1,
            }
            continue;
        }
        let next = lower[index..]
            .find('<')
            .map_or(lower.len(), |offset| index + offset);
        push_decoded(&mut out, &html[index..next]);
        index = next;
    }
    collapse_blank_lines(&out)
}

fn push_decoded(out: &mut String, text: &str) {
    let mut rest = text;
    while let Some(position) = rest.find('&') {
        out.push_str(&rest[..position]);
        let after = &rest[position..];
        match after.find(';').filter(|end| *end <= 10) {
            Some(end) => {
                out.push(decode_entity(&after[1..end]));
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
}

fn decode_entity(entity: &str) -> char {
    match entity {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        _ => {
            let numeric = entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| {
                    entity
                        .strip_prefix('#')
                        .and_then(|number| number.parse::<u32>().ok())
                });
            numeric.and_then(char::from_u32).unwrap_or(' ')
        }
    }
}

fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blanks = 0usize;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            blanks += 1;
            if blanks > 1 {
                continue;
            }
        } else {
            blanks = 0;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
    }
    out.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_must_be_http_and_keyless() {
        assert!(validate_url("https://example.com/doc").is_ok());
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("javascript:alert(1)").is_err());
        assert!(validate_url("https://user:pass@example.com").is_err());
        assert!(validate_url("https://").is_err());
    }

    #[derive(Debug)]
    struct Cause(&'static str);

    impl std::fmt::Display for Cause {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }

    impl std::error::Error for Cause {}

    #[derive(Debug)]
    struct Outer(Cause);

    impl std::fmt::Display for Outer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("error sending request")
        }
    }

    impl std::error::Error for Outer {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    #[test]
    fn an_error_names_its_cause() {
        let error = Outer(Cause("operation timed out"));
        assert_eq!(
            error_chain(&error),
            "error sending request: operation timed out"
        );
    }

    #[test]
    fn private_hosts_are_refused() {
        for host in [
            "localhost",
            "api.localhost",
            "printer.local",
            "127.0.0.1",
            "10.1.2.3",
            "192.168.3.132",
            "172.16.5.4",
            "169.254.1.1",
            "::1",
            "fc00::1",
            "fe80::1",
        ] {
            assert!(is_private_host(host), "{host}");
        }
        for host in ["example.com", "8.8.8.8", "1.1.1.1"] {
            assert!(!is_private_host(host), "{host}");
        }
    }

    #[test]
    fn html_is_reduced_to_text() {
        let html = "<!doctype html><html><head><style>body{color:red}</style>\
                    <script>alert('x')</script></head><body>\
                    <h1>Title &amp; more</h1><p>Hello <b>world</b>&nbsp;!</p>\
                    <ul><li>one</li><li>two</li></ul><!-- note --></body></html>";
        let text = html_to_text(html);
        assert!(text.contains("Title & more"), "{text}");
        assert!(text.contains("Hello world !"), "{text}");
        assert!(text.contains("one"), "{text}");
        assert!(!text.contains("alert"), "{text}");
        assert!(!text.contains("color:red"), "{text}");
        assert!(!text.contains("note"), "{text}");
    }

    #[test]
    fn results_render_titles_urls_and_summaries() {
        let results = vec![
            serde_json::json!({"title": "First", "url": "https://a.example", "summary": "short"}),
            serde_json::json!({"title": "Second", "url": "https://b.example", "snippet": "fallback"}),
        ];
        let text = render_results(&results);
        assert!(text.contains("1. First"), "{text}");
        assert!(text.contains("https://a.example"), "{text}");
        assert!(text.contains("short"), "{text}");
        assert!(text.contains("fallback"), "{text}");
    }

    #[test]
    fn the_connected_model_comes_before_the_configured_backends() {
        // An Ark model searches through its provider's hosted tool; the saved
        // DeepSeek key and the SearXNG URL only answer when that fails.
        let chain = WebBackend::chain(
            Some("https://ark.cn-beijing.volces.com/api/v3"),
            "deepseek-v4-1-flash-261017",
            "ark-key",
            Some("deepseek-key"),
            Some("http://192.168.3.132:8080"),
        );
        assert_eq!(chain.len(), 3, "hosted, then key, then SearXNG");
        assert!(matches!(
            &chain[0],
            WebBackend::Hosted { endpoint, model, .. }
                if endpoint == "https://ark.cn-beijing.volces.com/api/v3/responses"
                    && model == "deepseek-v4-1-flash-261017"
        ));
        assert!(matches!(&chain[1], WebBackend::Provider { .. }));
        assert!(matches!(&chain[2], WebBackend::Searxng { .. }));
    }

    #[test]
    fn a_deepseek_key_is_not_tried_twice() {
        let chain = WebBackend::chain(
            Some("https://api.deepseek.com/anthropic"),
            "deepseek-flash",
            "shared-key",
            Some("shared-key"),
            None,
        );
        assert_eq!(chain.len(), 1, "one service, one key: one attempt");
        assert!(matches!(&chain[0], WebBackend::Provider { .. }));
        assert_eq!(
            provider_tools_url("https://api.deepseek.com/v1", "search"),
            "https://api.deepseek.com/v1/tools/search"
        );
        // The chat base used by the Anthropic-compatible provider carries a
        // path; the tools live at the API root, not under it.
        assert_eq!(
            provider_tools_url("https://api.deepseek.com/anthropic", "search"),
            "https://api.deepseek.com/v1/tools/search"
        );
        assert_eq!(
            provider_tools_url("https://api.deepseek.com/anthropic/v1", "fetch"),
            "https://api.deepseek.com/v1/tools/fetch"
        );
    }

    #[test]
    fn a_local_model_still_falls_back_to_the_configured_search() {
        let chain = WebBackend::chain(
            Some("http://192.168.3.132:18081/v1"),
            "qwen3.8-flash-next-iq3_s",
            "local",
            Some("deepseek-key"),
            Some("http://192.168.3.132:8080/"),
        );
        assert_eq!(chain.len(), 2, "key, then SearXNG");
        assert!(matches!(&chain[0], WebBackend::Provider { .. }));
        assert!(matches!(
            &chain[1],
            WebBackend::Searxng { url } if url == "http://192.168.3.132:8080"
        ));
        assert!(WebBackend::chain(
            Some("http://192.168.3.132:18081/v1"),
            "qwen3.8-flash-next-iq3_s",
            "local",
            None,
            None
        )
        .is_empty());
        assert!(WebBackend::chain(None, "", "", None, Some("nonsense")).is_empty());
        let endpoint = searxng_search_url("http://192.168.3.132:8080", "rust async").unwrap();
        assert_eq!(endpoint.path(), "/search");
        assert!(endpoint.query().unwrap_or("").contains("format=json"));
    }

    #[test]
    fn a_hosted_answer_yields_its_citations() {
        let value = serde_json::json!({
            "output": [
                {
                    "type": "web_search_call",
                    "action": {
                        "type": "search",
                        "query": "rust async",
                        "sources": [
                            {"type": "url", "url": "https://doc.rust-lang.org/book", "title": "The Book"}
                        ]
                    }
                },
                {
                    "type": "message",
                    "content": [{
                        "type": "output_text",
                        "text": "Rust has async blocks.",
                        "annotations": [{
                            "type": "url_citation",
                            "url": "https://doc.rust-lang.org/book",
                            "title": "The Book"
                        }]
                    }]
                }
            ]
        });
        let mut results = Vec::new();
        let mut seen = HashSet::new();
        collect_sources(&value, &mut results, &mut seen);
        // The same source on the search call and the citation is one result.
        assert_eq!(results.len(), 1, "{results:?}");
        assert_eq!(results[0]["url"], "https://doc.rust-lang.org/book");
        assert_eq!(results[0]["title"], "The Book");
        assert_eq!(response_text(&value), "Rust has async blocks.");
    }

    #[test]
    fn a_hosted_answer_without_citations_still_reaches_the_model() {
        let value = serde_json::json!({
            "output": [{
                "type": "message",
                "content": [{"type": "output_text", "text": "No sources were cited."}]
            }]
        });
        let mut results = Vec::new();
        let mut seen = HashSet::new();
        collect_sources(&value, &mut results, &mut seen);
        assert!(results.is_empty());
        assert_eq!(response_text(&value), "No sources were cited.");
    }

    #[test]
    fn an_answer_result_renders_without_an_empty_url_line() {
        let results = vec![serde_json::json!({
            "title": "web search answer",
            "url": "",
            "summary": "Rust has async blocks."
        })];
        let text = render_results(&results);
        assert_eq!(text, "1. web search answer\n   Rust has async blocks.\n");
    }
}
