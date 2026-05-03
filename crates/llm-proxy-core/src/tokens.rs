use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub accepted_prediction_tokens: Option<u64>,
    pub rejected_prediction_tokens: Option<u64>,
    pub token_source: String,
}

pub fn estimate_tokens_from_chars(text: &str) -> u64 {
    let chars = text.chars().count() as u64;
    chars.div_ceil(4)
}

pub fn token_usage_from_provider(usage: &Value) -> Option<TokenUsage> {
    let input_tokens = usage_u64(usage, &["prompt_tokens", "input_tokens"]);
    let output_tokens = usage_u64(usage, &["completion_tokens", "output_tokens"]);
    let total_tokens = usage_u64(usage, &["total_tokens"]);
    if input_tokens.is_none() && output_tokens.is_none() && total_tokens.is_none() {
        return None;
    }

    let output_details = &["completion_tokens_details", "output_tokens_details"];

    Some(TokenUsage {
        input_tokens,
        output_tokens,
        total_tokens: total_tokens
            .or_else(|| Some(input_tokens.unwrap_or(0) + output_tokens.unwrap_or(0))),
        cached_input_tokens: nested_usage_u64(
            usage,
            &["prompt_tokens_details", "input_tokens_details"],
            &["cached_tokens"],
        ),
        reasoning_tokens: nested_usage_u64(usage, output_details, &["reasoning_tokens"]),
        accepted_prediction_tokens: nested_usage_u64(
            usage,
            output_details,
            &["accepted_prediction_tokens"],
        ),
        rejected_prediction_tokens: nested_usage_u64(
            usage,
            output_details,
            &["rejected_prediction_tokens"],
        ),
        token_source: "provider".to_owned(),
    })
}

pub fn estimate_token_usage(request: &Value, response: &Value) -> TokenUsage {
    let input_tokens = estimate_tokens_from_chars(&request.to_string());
    let output_tokens = estimate_tokens_from_chars(&response.to_string());
    TokenUsage {
        input_tokens: Some(input_tokens),
        output_tokens: Some(output_tokens),
        total_tokens: Some(input_tokens + output_tokens),
        cached_input_tokens: None,
        reasoning_tokens: None,
        accepted_prediction_tokens: None,
        rejected_prediction_tokens: None,
        token_source: "estimated_char_heuristic".to_owned(),
    }
}

fn usage_u64(usage: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| usage.get(*key)?.as_u64())
}

fn nested_usage_u64(usage: &Value, parents: &[&str], keys: &[&str]) -> Option<u64> {
    parents
        .iter()
        .find_map(|parent| usage.get(*parent))
        .and_then(|details| usage_u64(details, keys))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_with_ceiling() {
        assert_eq!(estimate_tokens_from_chars(""), 0);
        assert_eq!(estimate_tokens_from_chars("a"), 1);
        assert_eq!(estimate_tokens_from_chars("abcd"), 1);
        assert_eq!(estimate_tokens_from_chars("abcde"), 2);
    }

    #[test]
    fn parses_provider_usage_details() {
        let usage = serde_json::json!({
            "prompt_tokens": 10,
            "completion_tokens": 5,
            "total_tokens": 15,
            "prompt_tokens_details": { "cached_tokens": 4 },
            "completion_tokens_details": {
                "reasoning_tokens": 2,
                "accepted_prediction_tokens": 3,
                "rejected_prediction_tokens": 1
            }
        });

        let parsed = token_usage_from_provider(&usage).expect("provider usage");

        assert_eq!(parsed.input_tokens, Some(10));
        assert_eq!(parsed.output_tokens, Some(5));
        assert_eq!(parsed.total_tokens, Some(15));
        assert_eq!(parsed.cached_input_tokens, Some(4));
        assert_eq!(parsed.reasoning_tokens, Some(2));
        assert_eq!(parsed.accepted_prediction_tokens, Some(3));
        assert_eq!(parsed.rejected_prediction_tokens, Some(1));
    }
}
