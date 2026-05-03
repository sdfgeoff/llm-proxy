use llm_proxy_core::tokens::{estimate_token_usage, token_usage_from_provider, TokenUsage};
use serde_json::Value;

pub(crate) fn token_usage_for_response(
    request: &Value,
    response_body: &[u8],
) -> (Option<TokenUsage>, Option<String>) {
    let response_json = serde_json::from_slice::<Value>(response_body).ok();
    let provider_usage = response_json.as_ref().and_then(|value| value.get("usage"));
    let provider_usage_json = provider_usage.map(Value::to_string);
    let token_usage = provider_usage
        .and_then(token_usage_from_provider)
        .or_else(|| {
            let response_value = response_json.unwrap_or_else(|| {
                Value::String(String::from_utf8_lossy(response_body).into_owned())
            });
            Some(estimate_token_usage(request, &response_value))
        });

    (token_usage, provider_usage_json)
}
