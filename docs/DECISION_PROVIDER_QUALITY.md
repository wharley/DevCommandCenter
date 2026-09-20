# Jev decision quality

The `jev-v2` policy uses `noul` questions: each value is the probability that a
proposition is true. It is not a percentage of work completed. The legacy
`*ConfidenceThreshold` fields remain in the contract for compatibility, but
filters use only `noul`, never a separate confidence field.

References: [Noul](https://docs.typesafe.ai/primitives/noul) and
[atomic questions](https://docs.typesafe.ai/introduction).

## Behavior

- **Models:** considers the request, the first conversation message, and recent
  messages. Each excerpt preserves its beginning and end. Routing prioritizes
  quality based on the provider's model catalog descriptions. A switch requires
  meeting the suitability threshold and a margin of 0.10 over both the current
  model and competing candidates. Ties retain the current model; without a clear
  winner, the router abstains. Providers with dynamic catalogs remain unsupported
  for routing. The catalog does not verify account access or provide performance
  benchmarks.
- **Skills and memory:** asks one usefulness question per candidate. A valid
  rejection of all memories produces empty memory context; network or schema
  failures preserve the original retrieval results. Observe mode does not change
  the context.
- **Review:** after a turn finishes, evaluates request coverage, constraints,
  evidence supporting execution claims, and appropriate validation. Uses history
  preceding the turn, the response, tool records, and that turn's change
  snapshots. It does not use a live workspace diff that could belong to other
  work. Test commands and completed tool calls do not prove that tests passed.
- **Review policy:** every criterion must meet the threshold. The lowest signal
  displayed is a conservative policy rule, not a joint probability or a
  completeness score. Review remains advisory and does not automatically trigger
  another execution. The review button includes the original request and the
  evaluated criteria.
- **Tools:** activation and the risk threshold are independent of memory
  filtering. In Enforce mode, classifier errors preserve the existing additional
  block.
- **Invalid responses:** missing expected answers, incorrect types, or
  probabilities outside 0–1 produce explicit failures rather than zero scores.

Per-section and per-candidate limits prevent long requests from crowding out
candidates or the entire response. Truncation is identified in the context and
decision history. Missing snapshots and partial evidence must not be treated as
positive proof. Evidence selection is bounded; it does not replace code review,
tests, or independent fact verification.

## Initial thresholds and compatibility

| Decision point | New default | Effect of increasing it |
| --- | --- | --- |
| Memory | 0.65 | Selects fewer memories |
| Skills | 0.65 | Selects fewer skills |
| Models | 0.80 | Recommends fewer switches |
| Review | 0.80 per criterion | Flags more responses |
| Tool risk | 0.65 | Flags fewer actions |

These are starting hypotheses, not thresholds calibrated for DCC. Previously
saved values are preserved. Older configurations receive the tool guard control
with an independent threshold of 0.65. API keys and conversation content are not
stored in decision history; the `evaluation_json` column stores the policy
version, criterion/candidate identifiers, probabilities, and a truncation flag.
Rows created before the migration remain available without detailed evaluations.

## Evaluation with real tasks

Before claiming an accuracy improvement, collect an initial sample of 50–100
tasks covering Portuguese, follow-up requests, long contexts, simple tasks,
implementation work, failing tests, and unsupported claims. Manually label:

1. Which models are suitable (there may be more than one).
2. Which memories and skills are useful.
3. Which review criteria actually failed.
4. Which actions require risk review.

Use part of the sample to tune thresholds and reserve a separate part for
assessment. Measure false positives and negatives, abstentions, unnecessary
model switches, p50/p95 latency, and downstream outcomes for each decision point.
Compare task quality, not just agreement between classifiers. Record the policy
version and the returned model identifier because `jev-latest` can change.
Decision history supports inspection of the signals; "keep response" feedback
is not automatically a correctness label.

## Local checks

```sh
cargo test -p dcc-infra decision_ --lib
cargo test -p dev-command-center-tauri --lib decision_provider_settings
yarn workspace @dcc/desktop typecheck
```

HTTP tests use a local server with controlled responses. They verify the API
contract, failure handling, and policy application; they do not measure the
accuracy of the live Jev model.
