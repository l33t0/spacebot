# Model Routing

How Spacebot decides which LLM to use for each process.

## Why Route

Different processes have different needs. A channel talking to a user needs the best conversational model. A compaction worker summarizing old turns needs something fast and cheap. A coding worker needs strong tool use. Running everything on the same expensive model wastes money. Running everything on a cheap model degrades quality where it matters.

ClawRouter (OpenClaw plugin) solves this by analyzing prompt content with a 14-dimension keyword scorer at request time. It doesn't know what process type is making the request, so it has to infer complexity from the text. Spacebot doesn't have this problem — we know the process type, the task type, and the purpose at spawn time. Routing decisions are explicit, not inferred.

## Three Levels

### Level 1: Process-Type Defaults

Every process type has a default model. This is the primary routing mechanism and covers most of the optimization.

```toml
[routing]
channel = "anthropic/claude-sonnet-4"
branch = "anthropic/claude-sonnet-4"
worker = "anthropic/claude-haiku-4.5"
compactor = "google/gemini-2.5-flash"
cortex = "google/gemini-2.5-flash"
```

The rationale:

| Process | Why this model tier |
|---------|-------------------|
| Channel | User-facing. Needs best conversational quality, personality consistency. Worth the cost. |
| Branch | Thinks with the channel's full context. Same model maintains reasoning coherence. |
| Worker | Executes specific tasks. Cheaper model with strong tool use is sufficient. |
| Compactor | Summarization and memory extraction. Fast and cheap. No personality needed. |
| Cortex | System-level observation. Small context, simple signal processing. Cheapest tier. |

### Level 2: Task-Type Overrides

Workers are generic — "do this task." Different tasks benefit from different models. The channel or branch decides the task type when spawning a worker, and the routing config maps task types to models.

```toml
[routing.worker]
default = "anthropic/claude-haiku-4.5"
coding = "anthropic/claude-sonnet-4"
summarization = "google/gemini-2.5-flash"
memory_extraction = "google/gemini-2.5-flash"
shell = "anthropic/claude-haiku-4.5"
```

Task types are explicit strings passed at worker spawn time. The set of task types is open — operators can define their own and map them to models. Unknown task types fall back to `default`.

A branch can also optionally override its model if the channel decides the thinking task is particularly complex:

```toml
[routing.branch]
default = "anthropic/claude-sonnet-4"
deep_reasoning = "anthropic/claude-opus-4"
```

### Level 3: Fallback Chains

When a model fails (rate limit, downtime, billing error), try the next model in a configured fallback chain instead of failing the process.

```toml
[routing.fallback]
"anthropic/claude-sonnet-4" = ["anthropic/claude-haiku-4.5", "google/gemini-2.5-pro"]
"anthropic/claude-haiku-4.5" = ["google/gemini-2.5-flash"]
"google/gemini-2.5-flash" = ["anthropic/claude-haiku-4.5"]
"anthropic/claude-opus-4" = ["anthropic/claude-sonnet-4"]
```

Fallback is triggered on:
- HTTP 429 (rate limited)
- HTTP 502/503/504 (provider down)
- Connection timeout
- Provider-specific billing/auth errors

Fallback is NOT triggered on:
- Successful responses (even if the content is bad)
- HTTP 400 (bad request — our fault, not the provider's)
- Context length exceeded (route to a model with a bigger window instead)

Each attempt is logged. The fallback chain is tried in order, max 3 attempts total. If all fail, the error propagates to the caller.

Rate-limited models are deprioritized for a configurable cooldown period (default 60s) — subsequent routing decisions skip them as primary and prefer fallbacks.

## Configuration

### Where It Lives

Routing config follows the standard `env > DB > default` resolution:

1. **Environment variables** for quick overrides: `SPACEBOT_ROUTING_CHANNEL=anthropic/claude-opus-4`
2. **redb config** for persistent per-instance settings (set via CLI or settings API)
3. **Defaults** baked into the binary (sensible starting config)

### Schema

```rust
pub struct RoutingConfig {
    /// Model per process type.
    pub channel: String,
    pub branch: String,
    pub worker: String,
    pub compactor: String,
    pub cortex: String,

    /// Task-type overrides for workers.
    pub worker_overrides: HashMap<String, String>,

    /// Task-type overrides for branches.
    pub branch_overrides: HashMap<String, String>,

    /// Fallback chains per model.
    pub fallbacks: HashMap<String, Vec<String>>,

    /// How long to deprioritize a rate-limited model (seconds).
    pub rate_limit_cooldown_secs: u64,
}
```

### Defaults

```rust
impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            channel: "anthropic/claude-sonnet-4".into(),
            branch: "anthropic/claude-sonnet-4".into(),
            worker: "anthropic/claude-haiku-4.5".into(),
            compactor: "google/gemini-2.5-flash".into(),
            cortex: "google/gemini-2.5-flash".into(),
            worker_overrides: HashMap::from([
                ("coding".into(), "anthropic/claude-sonnet-4".into()),
                ("summarization".into(), "google/gemini-2.5-flash".into()),
                ("memory_extraction".into(), "google/gemini-2.5-flash".into()),
            ]),
            branch_overrides: HashMap::new(),
            fallbacks: HashMap::from([
                ("anthropic/claude-sonnet-4".into(), vec![
                    "anthropic/claude-haiku-4.5".into(),
                    "google/gemini-2.5-pro".into(),
                ]),
                ("anthropic/claude-haiku-4.5".into(), vec![
                    "google/gemini-2.5-flash".into(),
                ]),
                ("google/gemini-2.5-flash".into(), vec![
                    "anthropic/claude-haiku-4.5".into(),
                ]),
            ]),
            rate_limit_cooldown_secs: 60,
        }
    }
}
```

## How It Works in Code

### Model Resolution

`LlmManager` gains a `resolve_for_process()` method:

```rust
impl LlmManager {
    /// Resolve the model for a process type and optional task type.
    pub fn resolve_for_process(
        &self,
        process_type: ProcessType,
        task_type: Option<&str>,
    ) -> String {
        let config = &self.routing_config;

        let base_model = match process_type {
            ProcessType::Channel => &config.channel,
            ProcessType::Branch => &config.branch,
            ProcessType::Worker => &config.worker,
            ProcessType::Compactor => &config.compactor,
            ProcessType::Cortex => &config.cortex,
        };

        // Check for task-type override
        if let Some(task) = task_type {
            let overrides = match process_type {
                ProcessType::Worker => &config.worker_overrides,
                ProcessType::Branch => &config.branch_overrides,
                _ => return base_model.clone(),
            };
            if let Some(override_model) = overrides.get(task) {
                return override_model.clone();
            }
        }

        base_model.clone()
    }
}
```

### SpacebotModel Construction

When spawning a process, the model is resolved from config:

```rust
// Channel gets its configured model
let model_name = llm_manager.resolve_for_process(ProcessType::Channel, None);
let model = SpacebotModel::make(&llm_manager, &model_name);

// Worker gets task-type-specific model
let model_name = llm_manager.resolve_for_process(ProcessType::Worker, Some("coding"));
let model = SpacebotModel::make(&llm_manager, &model_name);

// Compactor gets the cheap model
let model_name = llm_manager.resolve_for_process(ProcessType::Compactor, None);
let model = SpacebotModel::make(&llm_manager, &model_name);
```

### Fallback on Error

Fallback wraps the `completion()` call in `SpacebotModel`:

```rust
async fn completion_with_fallback(
    &self,
    request: CompletionRequest,
) -> Result<CompletionResponse<RawResponse>, CompletionError> {
    // Try primary model
    match self.completion(request.clone()).await {
        Ok(response) => return Ok(response),
        Err(error) if is_retriable(&error) => {
            tracing::warn!(
                model = %self.model_name,
                %error,
                "primary model failed, trying fallback"
            );
        }
        Err(error) => return Err(error),
    }

    // Try fallback chain
    let fallbacks = self.llm_manager.get_fallbacks(&self.full_model_name());
    for fallback_model in fallbacks.iter().take(MAX_FALLBACK_ATTEMPTS) {
        let fallback = SpacebotModel::make(&self.llm_manager, fallback_model);
        match fallback.completion(request.clone()).await {
            Ok(response) => {
                tracing::info!(
                    original = %self.model_name,
                    fallback = %fallback_model,
                    "fallback model succeeded"
                );
                return Ok(response);
            }
            Err(error) if is_retriable(&error) => {
                tracing::warn!(fallback = %fallback_model, %error, "fallback also failed");
                continue;
            }
            Err(error) => return Err(error),
        }
    }

    Err(CompletionError::ProviderError(
        "all models in fallback chain failed".into()
    ))
}
```

### Rate Limit Tracking

`LlmManager` tracks rate-limited models with a simple time-based map:

```rust
pub struct LlmManager {
    config: LlmConfig,
    routing_config: RoutingConfig,
    http_client: reqwest::Client,
    rate_limited: Arc<RwLock<HashMap<String, Instant>>>,
}
```

When a 429 is received, the model is added with the current timestamp. `resolve_for_process()` checks this map and skips to the first fallback if the primary is cooling down.

## What We Don't Do

**No prompt-level content analysis.** ClawRouter's keyword scorer is solving a problem we don't have. We know the process type and task type at spawn time.

**No LLM classifier.** ClawRouter has a fallback LLM classifier for ambiguous requests. We don't need it — routing is deterministic from config.

**No per-request cost estimation.** ClawRouter estimates cost pre-request for wallet balance checks. Spacebot uses API keys directly, not a payment proxy. Cost tracking is a reporting concern, not a routing concern.

**No session pinning.** ClawRouter pins a model to a session for consistency. In Spacebot, each process already has a fixed model for its lifetime — the channel uses the same model across all turns, the worker uses the same model for all its iterations. This is inherent in the architecture.

**No agentic detection.** ClawRouter infers "agentic" tasks from prompt keywords. In Spacebot, workers are explicitly spawned for tasks — the channel knows it's spawning a coding worker vs a summarization worker. The task type is data, not inference.
