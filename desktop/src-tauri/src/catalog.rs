//! The model catalog: a maintained list of tool-capable models from
//! models.dev, filtered to the providers this host can actually drive.
//!
//! The remote JSON is large and changes often, so it is fetched by the backend,
//! reduced to the fields the picker needs and cached for a day. A small bundled
//! snapshot keeps first run usable when the network is not.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// The canonical catalog, then a byte-identical OpenCode mirror. The mirror
/// is metadata only; provider endpoints are fixed in `PROVIDERS`, so it cannot
/// redirect a key.
const SOURCE_URLS: &[&str] = &[
    "https://models.dev/api.json",
    "https://models.opencode.ai/api.json",
];
const SOURCE_URL_ENV: &str = "KEELSON_CATALOG_URL";
const CACHE_FILE: &str = "model-catalog.json";
const CACHE_VERSION: u32 = 4;
const CACHE_TTL_SECS: u64 = 60 * 60;
const FALLBACK: &str = include_str!("../catalog-fallback.json");

/// One provider the host knows how to call. The endpoint is fixed here rather
/// than taken from the remote JSON, so a bad catalog entry cannot redirect a
/// key to another host.
struct ProviderSpec {
    id: &'static str,
    name: &'static str,
    kind: &'static str,
    base_url: &'static str,
    key_env: &'static str,
}

const PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec {
        id: "anthropic",
        name: "Anthropic",
        kind: "anthropic-compatible",
        base_url: "https://api.anthropic.com",
        key_env: "ANTHROPIC_API_KEY",
    },
    ProviderSpec {
        id: "openai",
        name: "OpenAI",
        kind: "openai-compatible",
        base_url: "https://api.openai.com/v1",
        key_env: "OPENAI_API_KEY",
    },
    ProviderSpec {
        id: "google",
        name: "Google Gemini",
        kind: "openai-compatible",
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        key_env: "GEMINI_API_KEY",
    },
    ProviderSpec {
        id: "deepseek",
        name: "DeepSeek",
        kind: "openai-compatible",
        base_url: "https://api.deepseek.com",
        key_env: "DEEPSEEK_API_KEY",
    },
    ProviderSpec {
        id: "xai",
        name: "xAI",
        kind: "openai-compatible",
        base_url: "https://api.x.ai/v1",
        key_env: "XAI_API_KEY",
    },
    ProviderSpec {
        id: "moonshotai",
        name: "Moonshot AI",
        kind: "openai-compatible",
        base_url: "https://api.moonshot.ai/v1",
        key_env: "MOONSHOT_API_KEY",
    },
    ProviderSpec {
        id: "moonshotai-cn",
        name: "Moonshot AI (China)",
        kind: "openai-compatible",
        base_url: "https://api.moonshot.cn/v1",
        key_env: "MOONSHOT_API_KEY",
    },
    ProviderSpec {
        id: "zhipuai",
        name: "Zhipu AI",
        kind: "openai-compatible",
        base_url: "https://open.bigmodel.cn/api/paas/v4",
        key_env: "ZHIPU_API_KEY",
    },
    ProviderSpec {
        id: "alibaba",
        name: "Alibaba Qwen",
        kind: "openai-compatible",
        base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        key_env: "DASHSCOPE_API_KEY",
    },
    ProviderSpec {
        id: "alibaba-cn",
        name: "Alibaba Qwen (China)",
        kind: "openai-compatible",
        base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
        key_env: "DASHSCOPE_API_KEY",
    },
    ProviderSpec {
        id: "minimax",
        name: "MiniMax",
        kind: "anthropic-compatible",
        base_url: "https://api.minimax.io/anthropic/v1",
        key_env: "MINIMAX_API_KEY",
    },
    ProviderSpec {
        id: "minimax-cn",
        name: "MiniMax (China)",
        kind: "anthropic-compatible",
        base_url: "https://api.minimax.cn/anthropic/v1",
        key_env: "MINIMAX_API_KEY",
    },
    ProviderSpec {
        id: "mistral",
        name: "Mistral",
        kind: "openai-compatible",
        base_url: "https://api.mistral.ai/v1",
        key_env: "MISTRAL_API_KEY",
    },
    ProviderSpec {
        id: "groq",
        name: "Groq",
        kind: "openai-compatible",
        base_url: "https://api.groq.com/openai/v1",
        key_env: "GROQ_API_KEY",
    },
    ProviderSpec {
        id: "cerebras",
        name: "Cerebras",
        kind: "openai-compatible",
        base_url: "https://api.cerebras.ai/v1",
        key_env: "CEREBRAS_API_KEY",
    },
    ProviderSpec {
        id: "togetherai",
        name: "Together AI",
        kind: "openai-compatible",
        base_url: "https://api.together.xyz/v1",
        key_env: "TOGETHER_API_KEY",
    },
    ProviderSpec {
        id: "siliconflow",
        name: "SiliconFlow",
        kind: "openai-compatible",
        base_url: "https://api.siliconflow.com/v1",
        key_env: "SILICONFLOW_API_KEY",
    },
    ProviderSpec {
        id: "siliconflow-cn",
        name: "SiliconFlow (China)",
        kind: "openai-compatible",
        base_url: "https://api.siliconflow.cn/v1",
        key_env: "SILICONFLOW_CN_API_KEY",
    },
    ProviderSpec {
        id: "openrouter",
        name: "OpenRouter",
        kind: "openai-compatible",
        base_url: "https://openrouter.ai/api/v1",
        key_env: "OPENROUTER_API_KEY",
    },
    ProviderSpec {
        id: "fireworks-ai",
        name: "Fireworks AI",
        kind: "openai-compatible",
        base_url: "https://api.fireworks.ai/inference/v1",
        key_env: "FIREWORKS_API_KEY",
    },
    ProviderSpec {
        id: "huggingface",
        name: "Hugging Face",
        kind: "openai-compatible",
        base_url: "https://router.huggingface.co/v1",
        key_env: "HF_TOKEN",
    },
    ProviderSpec {
        id: "nvidia",
        name: "NVIDIA",
        kind: "openai-compatible",
        base_url: "https://integrate.api.nvidia.com/v1",
        key_env: "NVIDIA_API_KEY",
    },
    ProviderSpec {
        id: "novita-ai",
        name: "Novita AI",
        kind: "openai-compatible",
        base_url: "https://api.novita.ai/openai",
        key_env: "NOVITA_API_KEY",
    },
    ProviderSpec {
        id: "deepinfra",
        name: "DeepInfra",
        kind: "openai-compatible",
        base_url: "https://api.deepinfra.com/v1/openai",
        key_env: "DEEPINFRA_API_KEY",
    },
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalog {
    /// `models.dev`, `cache` or `fallback`.
    pub source: String,
    /// Unix seconds; zero for the bundled snapshot.
    pub fetched_at: u64,
    /// True when a refresh failed or the bundled snapshot was used.
    pub stale: bool,
    pub providers: Vec<CatalogProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogProvider {
    pub id: String,
    pub name: String,
    /// The wire protocol the host uses for this provider.
    pub kind: String,
    pub base_url: String,
    pub key_env: String,
    pub models: Vec<CatalogModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogModel {
    pub id: String,
    pub name: String,
    pub context: Option<u64>,
    pub output: Option<u64>,
    pub cost_input: Option<f64>,
    pub cost_output: Option<f64>,
    pub reasoning: bool,
    /// The model accepts image input (a capability, not an upload feature).
    #[serde(default)]
    pub vision: bool,
    /// The model accepts PDF input.
    #[serde(default)]
    pub pdf: bool,
    pub release_date: Option<String>,
    pub updated: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogCache {
    #[serde(default)]
    version: u32,
    fetched_at: u64,
    #[serde(default)]
    etag: Option<String>,
    providers: Vec<CatalogProvider>,
}

#[derive(Debug, Deserialize)]
struct RemoteCatalog(HashMap<String, RemoteProvider>);

#[derive(Debug, Default, Deserialize)]
struct RemoteProvider {
    #[serde(default)]
    models: HashMap<String, RemoteModel>,
}

#[derive(Debug, Default, Deserialize)]
struct RemoteModel {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    family: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    modalities: Option<RemoteModalities>,
    #[serde(default)]
    tool_call: bool,
    #[serde(default)]
    reasoning: bool,
    #[serde(default)]
    release_date: Option<String>,
    #[serde(default)]
    last_updated: Option<String>,
    #[serde(default)]
    limit: Option<RemoteLimit>,
    #[serde(default)]
    cost: Option<RemoteCost>,
}

#[derive(Debug, Default, Deserialize)]
struct RemoteModalities {
    #[serde(default)]
    input: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RemoteLimit {
    #[serde(default)]
    context: Option<u64>,
    #[serde(default)]
    output: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct RemoteCost {
    #[serde(default)]
    input: Option<f64>,
    #[serde(default)]
    output: Option<f64>,
}

/// What the picker can show right now: the cache, fresh or stale, or the
/// bundled snapshot. This never touches the network, so opening settings
/// lists models immediately; `load(.., true)` refreshes behind it.
pub fn cached(cache_dir: &Path) -> Result<ModelCatalog, String> {
    let cache_path = cache_dir.join(CACHE_FILE);
    if let Some(cache) = read_cache(&cache_path) {
        let fresh = now().saturating_sub(cache.fetched_at) < CACHE_TTL_SECS;
        return Ok(from_cache(&cache, !fresh));
    }
    match fallback() {
        Some(cache) => Ok(ModelCatalog {
            source: "fallback".to_owned(),
            fetched_at: 0,
            stale: true,
            providers: cache.providers,
        }),
        None => Err("还没有模型目录缓存；请联网刷新一次".to_owned()),
    }
}

/// Returns a live fetch or revalidation when `refresh` is set, and the cached
/// list otherwise (see `cached`). A failed refresh falls back to the cache or
/// the bundled snapshot.
pub async fn load(cache_dir: &Path, refresh: bool) -> Result<ModelCatalog, String> {
    let cache_path = cache_dir.join(CACHE_FILE);
    if !refresh {
        return cached(cache_dir);
    }
    let cached = read_cache(&cache_path);

    let etag = cached.as_ref().and_then(|cache| cache.etag.as_deref());
    match fetch(etag).await {
        Ok(FetchOutcome::Fresh { providers, etag }) => {
            let fetched_at = now();
            let cache = CatalogCache {
                version: CACHE_VERSION,
                fetched_at,
                etag,
                providers: providers.clone(),
            };
            let _ = write_cache(&cache_path, &cache);
            Ok(ModelCatalog {
                source: "models.dev".to_owned(),
                fetched_at,
                stale: false,
                providers,
            })
        }
        Ok(FetchOutcome::NotModified) => {
            let Some(mut cache) = cached else {
                return Err("models.dev returned 304 without a cached catalog".to_owned());
            };
            cache.fetched_at = now();
            let _ = write_cache(&cache_path, &cache);
            Ok(from_cache(&cache, false))
        }
        Err(error) => {
            if let Some(cache) = cached {
                return Ok(from_cache(&cache, true));
            }
            match fallback() {
                Some(cache) => Ok(ModelCatalog {
                    source: "fallback".to_owned(),
                    fetched_at: 0,
                    stale: true,
                    providers: cache.providers,
                }),
                None => Err(format!("无法获取模型列表：{error}")),
            }
        }
    }
}

enum FetchOutcome {
    Fresh {
        providers: Vec<CatalogProvider>,
        etag: Option<String>,
    },
    NotModified,
}

async fn fetch(etag: Option<&str>) -> Result<FetchOutcome, String> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("keelson/", env!("CARGO_PKG_VERSION")))
        // A blocked host should fail fast so the mirror is tried promptly.
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    let mut errors = Vec::new();
    for url in sources() {
        match fetch_one(&client, &url, etag).await {
            Ok(outcome) => return Ok(outcome),
            Err(error) => errors.push(format!("{url}: {error}")),
        }
    }
    Err(errors.join("; "))
}

async fn fetch_one(
    client: &reqwest::Client,
    url: &str,
    etag: Option<&str>,
) -> Result<FetchOutcome, String> {
    let mut request = client.get(url);
    if let Some(etag) = etag {
        request = request.header(reqwest::header::IF_NONE_MATCH, etag);
    }
    let response = request.send().await.map_err(|error| error.to_string())?;
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(FetchOutcome::NotModified);
    }
    if !response.status().is_success() {
        return Err(format!("returned {}", response.status()));
    }
    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let remote: RemoteCatalog = response
        .json()
        .await
        .map_err(|error| format!("invalid JSON: {error}"))?;
    let providers = filter(&remote);
    if providers.is_empty() {
        return Err("no usable models".to_owned());
    }
    Ok(FetchOutcome::Fresh { providers, etag })
}

fn sources() -> Vec<String> {
    sources_with(std::env::var(SOURCE_URL_ENV).ok().as_deref())
}

fn sources_with(over: Option<&str>) -> Vec<String> {
    let mut urls = Vec::new();
    if let Some(url) = over.map(str::trim).filter(|url| !url.is_empty()) {
        urls.push(url.to_owned());
    }
    for url in SOURCE_URLS {
        if !urls.iter().any(|existing| existing == url) {
            urls.push((*url).to_owned());
        }
    }
    urls
}

fn from_cache(cache: &CatalogCache, stale: bool) -> ModelCatalog {
    ModelCatalog {
        source: "cache".to_owned(),
        fetched_at: cache.fetched_at,
        stale,
        providers: cache.providers.clone(),
    }
}

fn filter(remote: &RemoteCatalog) -> Vec<CatalogProvider> {
    let mut providers = Vec::new();
    for spec in PROVIDERS {
        let Some(remote_provider) = remote.0.get(spec.id) else {
            continue;
        };

        // One entry per family: the newest current model. Deprecated models
        // are never shown; beta and alpha stay, because they are new, not old.
        let mut latest: HashMap<&str, (&String, &RemoteModel)> = HashMap::new();
        for (id, model) in &remote_provider.models {
            if id.trim().is_empty() || !model.tool_call {
                continue;
            }
            if model.status.as_deref() == Some("deprecated") {
                continue;
            }
            let family = model.family.as_deref().unwrap_or(id.as_str());
            let replace = match latest.get(family) {
                None => true,
                Some((current_id, current)) => newer(id, model, current_id, current),
            };
            if replace {
                latest.insert(family, (id, model));
            }
        }

        let mut models: Vec<CatalogModel> = latest
            .into_values()
            .map(|(id, model)| {
                let inputs = model
                    .modalities
                    .as_ref()
                    .map(|modalities| modalities.input.as_slice())
                    .unwrap_or(&[]);
                CatalogModel {
                    id: id.clone(),
                    name: model
                        .name
                        .clone()
                        .filter(|name| !name.trim().is_empty())
                        .unwrap_or_else(|| id.clone()),
                    context: model.limit.as_ref().and_then(|limit| limit.context),
                    output: model.limit.as_ref().and_then(|limit| limit.output),
                    cost_input: model.cost.as_ref().and_then(|cost| cost.input),
                    cost_output: model.cost.as_ref().and_then(|cost| cost.output),
                    reasoning: model.reasoning,
                    vision: inputs
                        .iter()
                        .any(|input| input == "image" || input.starts_with("image/")),
                    pdf: inputs
                        .iter()
                        .any(|input| input == "pdf" || input == "application/pdf"),
                    release_date: model.release_date.clone(),
                    updated: model.last_updated.clone(),
                }
            })
            .collect();
        models.sort_by(|a, b| {
            b.release_date
                .cmp(&a.release_date)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.id.cmp(&b.id))
        });
        trim_old(&mut models);
        if models.is_empty() {
            continue;
        }
        providers.push(CatalogProvider {
            id: spec.id.to_owned(),
            name: spec.name.to_owned(),
            kind: spec.kind.to_owned(),
            base_url: spec.base_url.to_owned(),
            key_env: spec.key_env.to_owned(),
            models,
        });
    }
    providers
}

fn newer(id: &str, model: &RemoteModel, current_id: &str, current: &RemoteModel) -> bool {
    (
        model.release_date.as_deref(),
        model.last_updated.as_deref(),
        id,
    ) > (
        current.release_date.as_deref(),
        current.last_updated.as_deref(),
        current_id,
    )
}

/// Keeps the catalog to current work: six months back from the provider's
/// newest family. An undated model is kept rather than guessed away.
fn trim_old(models: &mut Vec<CatalogModel>) {
    let newest = models
        .iter()
        .filter_map(|model| model.release_date.as_deref().and_then(month_index))
        .max();
    let Some(newest) = newest else {
        return;
    };
    models.retain(|model| {
        model
            .release_date
            .as_deref()
            .and_then(month_index)
            .is_none_or(|index| newest - index <= 6)
    });
}

fn month_index(date: &str) -> Option<i32> {
    let mut parts = date.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: i32 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&month) {
        return None;
    }
    Some(year * 12 + month)
}

fn fallback() -> Option<CatalogCache> {
    serde_json::from_str(FALLBACK).ok()
}

fn read_cache(path: &Path) -> Option<CatalogCache> {
    let cache: CatalogCache = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())?;
    (cache.version == CACHE_VERSION).then_some(cache)
}

fn write_cache(path: &Path, cache: &CatalogCache) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string(cache).map_err(|error| error.to_string())?;
    fs::write(path, text).map_err(|error| error.to_string())
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
    {
      "deepseek": {
        "models": {
          "deepseek-chat": {
            "name": "DeepSeek Chat",
            "tool_call": true,
            "reasoning": false,
            "release_date": "2026-08-01",
            "modalities": {"input": ["text", "image/png", "application/pdf"]},
            "limit": {"context": 131072, "output": 8192},
            "cost": {"input": 0.28, "output": 0.42}
          },
          "deepseek-embedding": {
            "name": "Embedding",
            "tool_call": false
          }
        }
      },
      "unknown-gateway": {
        "models": {
          "hidden": {"name": "Hidden", "tool_call": true}
        }
      }
    }
    "#;

    #[test]
    fn filters_curated_providers_and_tool_models() {
        let remote: RemoteCatalog = serde_json::from_str(SAMPLE).expect("a sample catalog");

        let providers = filter(&remote);

        assert_eq!(providers.len(), 1);
        let provider = &providers[0];
        assert_eq!(provider.id, "deepseek");
        assert_eq!(provider.kind, "openai-compatible");
        assert_eq!(provider.models.len(), 1);
        let model = &provider.models[0];
        assert_eq!(model.id, "deepseek-chat");
        assert_eq!(model.context, Some(131072));
        assert_eq!(model.cost_input, Some(0.28));
        assert!(model.vision);
        assert!(model.pdf);
    }

    #[test]
    fn deprecated_and_older_family_models_are_hidden() {
        let remote: RemoteCatalog = serde_json::from_str(
            r#"{"deepseek":{"models":{
              "old-flash":{"name":"Old Flash","family":"flash","tool_call":true,"release_date":"2026-01-01"},
              "new-flash":{"name":"New Flash","family":"flash","tool_call":true,"release_date":"2026-10-01"},
              "retired":{"name":"Retired","family":"pro","tool_call":true,"status":"deprecated","release_date":"2026-09-01"},
              "current-pro":{"name":"Current Pro","family":"pro","tool_call":true,"release_date":"2026-05-01"},
              "standalone":{"name":"Standalone","tool_call":true,"release_date":"2026-06-01"},
              "old-standalone":{"name":"Old Standalone","tool_call":true,"release_date":"2025-06-01"}
            }}}"#,
        )
        .expect("a sample catalog");

        let providers = filter(&remote);
        let ids = providers[0]
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["new-flash", "standalone", "current-pro"]);
    }

    #[test]
    fn models_are_newest_first() {
        let remote: RemoteCatalog = serde_json::from_str(
            r#"{"deepseek":{"models":{
              "old":{"name":"Old","tool_call":true,"release_date":"2026-06-01"},
              "new":{"name":"New","tool_call":true,"release_date":"2026-09-01"}
            }}}"#,
        )
        .expect("a sample catalog");

        let providers = filter(&remote);

        assert_eq!(
            providers[0]
                .models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["new", "old"]
        );
    }

    #[test]
    fn a_configured_mirror_comes_first() {
        let urls = sources_with(Some(" https://mirror.example/api.json "));

        assert_eq!(urls[0], "https://mirror.example/api.json");
        assert!(urls.iter().any(|url| url == "https://models.dev/api.json"));
        assert!(urls
            .iter()
            .any(|url| url == "https://models.opencode.ai/api.json"));

        let official = sources_with(Some("https://models.dev/api.json"));
        assert_eq!(
            official
                .iter()
                .filter(|url| *url == "https://models.dev/api.json")
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn a_cached_list_is_returned_without_the_network() {
        let dir = std::env::temp_dir().join(format!(
            "keelson-catalog-cached-{}-{}",
            now(),
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("a cache dir");
        let cache = CatalogCache {
            version: CACHE_VERSION,
            fetched_at: 1,
            etag: None,
            providers: fallback().expect("the fallback parses").providers,
        };
        write_cache(&dir.join(CACHE_FILE), &cache).expect("a cache file");

        let value = load(&dir, false).await.expect("the cached list");
        assert_eq!(value.source, "cache");
        assert!(value.stale);
        assert!(!value.providers.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_bundled_fallback_is_usable() {
        let cache = fallback().expect("the fallback parses");

        assert_eq!(cache.version, CACHE_VERSION);
        assert!(!cache.providers.is_empty());
        assert!(cache
            .providers
            .iter()
            .all(|provider| !provider.models.is_empty()));
    }

    #[test]
    fn a_cache_from_another_filter_is_ignored() {
        let path = std::env::temp_dir().join(format!(
            "keelson-catalog-{}-{}.json",
            now(),
            std::process::id()
        ));
        let body = r#"{"version":1,"fetchedAt":1,"providers":[]}"#;
        fs::write(&path, body).expect("a cache file");

        assert!(read_cache(&path).is_none());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn a_current_cache_keeps_its_etag() {
        let path = std::env::temp_dir().join(format!(
            "keelson-catalog-current-{}-{}.json",
            now(),
            std::process::id()
        ));
        let body = format!(
            r#"{{"version":{},"fetchedAt":1,"etag":"\"abc\"","providers":[]}}"#,
            CACHE_VERSION
        );
        fs::write(&path, body).expect("a cache file");

        let cache = read_cache(&path).expect("the current cache is read");
        assert_eq!(cache.etag.as_deref(), Some("\"abc\""));
        let _ = fs::remove_file(&path);
    }
}
