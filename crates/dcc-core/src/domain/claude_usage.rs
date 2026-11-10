//! Claude Code reports, at the end of each turn, what the whole session has
//! used so far (`modelUsage`, kept across resumes), not what the turn used.
//! A turn's share is what grew since the last report; a report smaller in
//! any counter means the session started counting again, and is the turn's
//! own.

use super::usage::ModelTokenUsage;

/// The turn's share of `current`, given the session's last report `mark`
/// for the same model.
pub fn turn_share(mark: Option<&ModelTokenUsage>, current: &ModelTokenUsage) -> ModelTokenUsage {
    let Some(mark) = mark.filter(|mark| grew_from(mark, current)) else {
        return current.clone();
    };
    let input_tokens = current.input_tokens - mark.input_tokens;
    let output_tokens = current.output_tokens - mark.output_tokens;
    let cached_input_tokens = current.cached_input_tokens - mark.cached_input_tokens;
    let cache_write_input_tokens = current.cache_write_input_tokens - mark.cache_write_input_tokens;
    let reasoning_output_tokens = current
        .reasoning_output_tokens
        .saturating_sub(mark.reasoning_output_tokens);
    ModelTokenUsage {
        model: current.model.clone(),
        input_tokens,
        output_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        reasoning_output_tokens,
        total_tokens: input_tokens
            .saturating_add(output_tokens)
            .saturating_add(cached_input_tokens)
            .saturating_add(cache_write_input_tokens),
        cost_usd: match (current.cost_usd, mark.cost_usd) {
            (Some(now), Some(before)) if now >= before => Some(now - before),
            (now, _) => now,
        },
    }
}

/// `share` added to what the same turn already stored (a turn can end more
/// than one Claude response when DCC hands it a message mid-turn).
pub fn add_to_turn(stored: Option<&ModelTokenUsage>, share: ModelTokenUsage) -> ModelTokenUsage {
    let Some(stored) = stored else {
        return share;
    };
    ModelTokenUsage {
        model: share.model,
        input_tokens: stored.input_tokens + share.input_tokens,
        output_tokens: stored.output_tokens + share.output_tokens,
        cached_input_tokens: stored.cached_input_tokens + share.cached_input_tokens,
        cache_write_input_tokens: stored.cache_write_input_tokens + share.cache_write_input_tokens,
        reasoning_output_tokens: stored.reasoning_output_tokens + share.reasoning_output_tokens,
        total_tokens: stored.total_tokens + share.total_tokens,
        cost_usd: match (stored.cost_usd, share.cost_usd) {
            (Some(left), Some(right)) => Some(left + right),
            (left, right) => left.or(right),
        },
    }
}

fn grew_from(mark: &ModelTokenUsage, current: &ModelTokenUsage) -> bool {
    current.input_tokens >= mark.input_tokens
        && current.output_tokens >= mark.output_tokens
        && current.cached_input_tokens >= mark.cached_input_tokens
        && current.cache_write_input_tokens >= mark.cache_write_input_tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(input: u64, output: u64, cached: u64, written: u64) -> ModelTokenUsage {
        ModelTokenUsage {
            model: Some("claude-opus-5-5".into()),
            input_tokens: input,
            output_tokens: output,
            cached_input_tokens: cached,
            cache_write_input_tokens: written,
            total_tokens: input + output + cached + written,
            ..Default::default()
        }
    }

    #[test]
    fn a_turn_counts_what_grew_since_the_last_report() {
        // The Reviewer's "bom dia": the session's totals went up a little.
        let before = usage(4647, 9364, 7_996_695, 3_917_253);
        let now = usage(4651, 9602, 9_122_782, 3_918_396);
        let share = turn_share(Some(&before), &now);
        assert_eq!(share.input_tokens, 4);
        assert_eq!(share.output_tokens, 238);
        assert_eq!(share.cached_input_tokens, 1_126_087);
        assert_eq!(share.cache_write_input_tokens, 1_143);
        assert_eq!(share.total_tokens, 4 + 238 + 1_126_087 + 1_143);
    }

    #[test]
    fn a_second_response_in_one_turn_adds_to_it() {
        let first = usage(1, 2, 30, 4);
        let second = usage(1, 1, 10, 0);
        let both = add_to_turn(Some(&first), second);
        assert_eq!(both.cached_input_tokens, 40);
        assert_eq!(both.total_tokens, 37 + 12);
    }

    #[test]
    fn a_session_counting_again_reports_its_own_turn() {
        let before = usage(53_765, 72_819, 5_760_582, 228_596);
        let now = usage(18, 6_135, 410_712, 60_281);
        assert_eq!(turn_share(Some(&before), &now), now);
        assert_eq!(turn_share(None, &now), now);
    }
}
