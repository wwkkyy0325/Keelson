//! Settings shared by the live evals.
//!
//! The default is the DeepSeek endpoint the evals were written against. Point
//! them at another OpenAI-compatible server with `KEELSON_EVAL_BASE_URL`,
//! `KEELSON_EVAL_MODEL`, `KEELSON_EVAL_API_KEY` and `KEELSON_EVAL_CONTEXT`.
//!
//! Only `DEEPSEEK_API_KEY` is accepted for the default endpoint. An
//! `ANTHROPIC_AUTH_TOKEN` is never sent to another provider.

use keelson::engine::ModelOverride;

/// The model a live eval drives, or `None` when no endpoint is configured.
pub fn eval_model() -> Option<ModelOverride> {
    if let Ok(base_url) = std::env::var("KEELSON_EVAL_BASE_URL") {
        let model = std::env::var("KEELSON_EVAL_MODEL").unwrap_or_else(|_| "q2_0".to_owned());
        let api_key =
            std::env::var("KEELSON_EVAL_API_KEY").unwrap_or_else(|_| "not-needed".to_owned());
        let context_window = std::env::var("KEELSON_EVAL_CONTEXT")
            .ok()
            .and_then(|value| value.trim().parse().ok());
        return Some(ModelOverride {
            provider: "openai-compatible".to_owned(),
            model,
            base_url: Some(base_url),
            api_key,
            context_window,
        });
    }
    let key = std::env::var("DEEPSEEK_API_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty())?;
    Some(ModelOverride {
        provider: "openai-compatible".to_owned(),
        model: "deepseek-flash".to_owned(),
        base_url: Some("https://api.deepseek.com".to_owned()),
        api_key: key,
        context_window: Some(1_048_576),
    })
}
