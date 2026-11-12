//! Estimated list prices of the models Claude Code and Codex run, in US
//! dollars per million tokens, for the usage dashboard's "Custo est.".
//!
//! ESTIMATES, not invoices: public API list prices as known on 2026-11-12
//! (Anthropic's model table cached 2026-11-03; OpenAI's GPT-5.x prices from
//! public reports after the 2026-07-30 GPT-5.6 cut, GPT-5.6 Sol at its list
//! price, not the promotion that ends 2026-11-21). Subscriptions (Claude
//! Max, ChatGPT Pro) are not billed per token, so this is what the same
//! traffic would cost on the API. Anthropic's cache write is the 5-minute
//! one (1.25x input); OpenAI bills no cache write, so it costs as input.
//! A model missing here has no price: callers count it as unknown, never
//! as zero.

/// Dollars per million tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelPrice {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

const fn anthropic(input: f64, output: f64, cache_read: f64) -> ModelPrice {
    ModelPrice {
        input,
        output,
        cache_read,
        cache_write: input * 1.25,
    }
}

const fn openai(input: f64, output: f64, cache_read: f64) -> ModelPrice {
    ModelPrice {
        input,
        output,
        cache_read,
        cache_write: input,
    }
}

/// Model ids, most specific first. An id matches its entry exactly or
/// with a named variant after it (`gpt-5.2-codex`), never with a further
/// version (`claude-opus-5-9`, `gpt-5.7` stay unknown).
const PRICES: &[(&str, ModelPrice)] = &[
    // Anthropic
    ("claude-fable-5-2", anthropic(8.0, 40.0, 0.20)),
    ("claude-mythos-5-2", anthropic(8.0, 40.0, 0.20)),
    ("claude-fable-5-1", anthropic(10.0, 50.0, 1.0)),
    ("claude-mythos-5-1", anthropic(10.0, 50.0, 1.0)),
    ("claude-fable-5", anthropic(10.0, 50.0, 1.0)),
    ("claude-mythos", anthropic(10.0, 50.0, 1.0)),
    ("claude-opus-5-5", anthropic(4.0, 20.0, 0.20)),
    ("claude-opus-5", anthropic(5.0, 25.0, 0.50)),
    ("claude-opus-4-8", anthropic(5.0, 25.0, 0.50)),
    ("claude-opus-4-7", anthropic(5.0, 25.0, 0.50)),
    ("claude-opus-4-6", anthropic(5.0, 25.0, 0.50)),
    ("claude-opus-4-5", anthropic(5.0, 25.0, 0.50)),
    ("claude-opus-4-1", anthropic(15.0, 75.0, 1.50)),
    ("claude-opus-4", anthropic(15.0, 75.0, 1.50)),
    ("claude-sonnet-5-5", anthropic(2.0, 10.0, 0.10)),
    ("claude-sonnet-5", anthropic(2.0, 10.0, 0.20)),
    ("claude-sonnet-4-6", anthropic(3.0, 15.0, 0.30)),
    ("claude-sonnet-4-5", anthropic(3.0, 15.0, 0.30)),
    ("claude-sonnet-4", anthropic(3.0, 15.0, 0.30)),
    ("claude-3-7-sonnet", anthropic(3.0, 15.0, 0.30)),
    ("claude-haiku-5-5", anthropic(0.10, 0.50, 0.01)),
    ("claude-haiku-4-5", anthropic(1.0, 5.0, 0.10)),
    ("claude-3-5-haiku", anthropic(0.80, 4.0, 0.08)),
    // OpenAI (Codex). "-codex" variants cost as their base model. The
    // GPT-6 prices come from third-party reports only (low confidence);
    // GPT-6.1 had no reliable source and stays unknown.
    (
        "gpt-6-astra",
        ModelPrice {
            input: 10.0,
            output: 50.0,
            cache_read: 1.0,
            cache_write: 12.5,
        },
    ),
    ("gpt-6-sol", openai(2.0, 10.0, 0.20)),
    ("gpt-6-luna", openai(0.10, 0.50, 0.01)),
    ("gpt-5.6-sol", openai(5.0, 30.0, 0.50)),
    ("gpt-5.6-terra", openai(2.0, 12.0, 0.20)),
    ("gpt-5.6-luna", openai(0.20, 1.20, 0.02)),
    ("gpt-5.5", openai(5.0, 30.0, 0.50)),
    ("gpt-5.4", openai(2.50, 15.0, 0.25)),
    ("gpt-5.3", openai(1.75, 14.0, 0.175)),
    ("gpt-5.2", openai(1.75, 14.0, 0.175)),
    ("gpt-5.1-codex-mini", openai(0.25, 2.0, 0.025)),
    ("gpt-5-codex-mini", openai(0.25, 2.0, 0.025)),
    ("gpt-5-mini", openai(0.25, 2.0, 0.025)),
    ("gpt-5-nano", openai(0.05, 0.40, 0.005)),
    ("gpt-5.1", openai(1.25, 10.0, 0.125)),
    ("gpt-5", openai(1.25, 10.0, 0.125)),
];

/// The model id without a provider prefix, a context suffix (`[1m]`) or a
/// date snapshot (`-20251001`), lowercased.
fn normalize(model: &str) -> String {
    let mut id = model.trim().to_ascii_lowercase();
    if let Some(at) = id.find('[') {
        id.truncate(at);
    }
    for prefix in [
        "us.anthropic.",
        "eu.anthropic.",
        "anthropic.",
        "openai/",
        "anthropic/",
    ] {
        if let Some(rest) = id.strip_prefix(prefix) {
            id = rest.to_string();
        }
    }
    if let Some((head, _)) = id.split_once('@') {
        id = head.to_string();
    }
    if let Some((head, tail)) = id.rsplit_once('-') {
        if tail.len() == 8 && tail.bytes().all(|byte| byte.is_ascii_digit()) {
            id = head.to_string();
        }
    }
    id
}

/// The price of `model`, or `None` when it is not in the table.
pub fn model_price(model: &str) -> Option<ModelPrice> {
    let id = normalize(model);
    PRICES
        .iter()
        .find(|(prefix, _)| {
            id == *prefix
                || id
                    .strip_prefix(prefix)
                    .and_then(|rest| rest.strip_prefix('-'))
                    .is_some_and(|rest| !rest.starts_with(|c: char| c.is_ascii_digit()))
        })
        .map(|(_, price)| *price)
}

/// The dollars of one call: fresh input, cache reads and writes, and
/// output (reasoning and thinking are billed as output and already in it).
pub fn call_cost(
    price: &ModelPrice,
    input: u64,
    cache_read: u64,
    cache_write: u64,
    output: u64,
) -> f64 {
    (input as f64 * price.input
        + cache_read as f64 * price.cache_read
        + cache_write as f64 * price.cache_write
        + output as f64 * price.output)
        / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_match_through_prefixes_snapshots_and_variants() {
        assert_eq!(model_price("claude-opus-5-5").unwrap().input, 4.0);
        assert_eq!(model_price("claude-opus-5").unwrap().input, 5.0);
        assert_eq!(model_price("claude-opus-4-1-20250805").unwrap().input, 15.0);
        assert_eq!(model_price("claude-haiku-4-5-20251001").unwrap().input, 1.0);
        assert_eq!(model_price("claude-opus-5-5[1m]").unwrap().output, 20.0);
        assert_eq!(
            model_price("us.anthropic.claude-sonnet-5-5")
                .unwrap()
                .cache_read,
            0.10
        );
        assert_eq!(model_price("gpt-5.2-codex").unwrap().input, 1.75);
        assert_eq!(model_price("gpt-5.6-sol").unwrap().output, 30.0);
        assert_eq!(model_price("gpt-5.1-codex-mini").unwrap().input, 0.25);
        assert_eq!(model_price("gpt-5").unwrap().input, 1.25);
        // "gpt-5" must not swallow "gpt-50" nor the unknown 6.x line.
        assert!(model_price("gpt-50").is_none());
        assert_eq!(model_price("gpt-6-astra").unwrap().cache_write, 12.5);
        assert!(model_price("gpt-6.1-sol").is_none());
        assert!(model_price("gpt-5.7").is_none());
        assert!(model_price("claude-opus-5-9").is_none());
        assert!(model_price("<synthetic>").is_none());
    }

    #[test]
    fn a_call_costs_each_bucket_at_its_rate() {
        let price = model_price("claude-opus-5-5").unwrap();
        // 1M fresh in ($4) + 1M cache read ($0.20) + 1M write ($5) + 1M out ($20)
        let cost = call_cost(&price, 1_000_000, 1_000_000, 1_000_000, 1_000_000);
        assert!((cost - 29.2).abs() < 1e-9);
    }
}
