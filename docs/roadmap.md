# MVP Roadmap

Tracking progress toward a working Spacebot that can hold a conversation, delegate work, manage memory, and connect to at least one messaging platform.

For each piece: reference IronClaw, OpenClaw, Nanobot, and Rig for inspiration, but make design decisions that align with Spacebot's architecture. Don't copy patterns that assume a monolithic session model.

---

## Current State

**What exists and compiles (zero errors, warnings only):**
- Project structure — all modules declared, module root pattern (`src/memory.rs` not `mod.rs`)
- Error hierarchy — thiserror domain enums (`ConfigError`, `DbError`, `LlmError`, `MemoryError`, `AgentError`, `SecretsError`) wrapped by top-level `Error` with `#[from]`
- Config loading — env-based with compaction/channel defaults, data dir setup
- Database connections — SQLite (sqlx) + LanceDB + redb. SQLite migrations for all tables (memories, associations, conversations, heartbeats). Migration runner in `db.rs`.
- LLM — `SpacebotModel` implements Rig's `CompletionModel` trait (completion, make, stream stub). Routes through `LlmManager` via direct HTTP to Anthropic and OpenAI. Handles tool definitions in requests and tool calls in responses.
- Memory — types (`Memory`, `Association`, `MemoryType`, `RelationType`), SQLite store (full CRUD + associations), LanceDB embedding storage + vector search (cosine) + FTS (Tantivy), fastembed (all-MiniLM-L6-v2, 384 dims), hybrid search (vector + FTS + graph traversal + RRF fusion), `MemorySearch` bundles store + lance + embedder. Maintenance (decay/prune stubs).
- Agent structs — Channel (278 lines, event loop with `tokio::select!`, branch/worker spawning, status block), Branch (107 lines, history clone, recall, conclusion return), Worker (155 lines, state machine with `can_transition_to`), Compactor (141 lines, tiered thresholds), Cortex (117 lines, signal processing). Core LLM calls within agents are simulated — the surrounding infrastructure is real.
- StatusBlock — event-driven updates from `ProcessEvent`, renders to context string
- SpacebotHook — tool start/complete events, leak detection regexes (`LazyLock`), status updates
- Messaging — `Messaging` trait with RPITIT + `MessagingDyn` companion + blanket impl. `MessagingManager` with adapter registry. Discord/Telegram/Webhook adapters are empty stubs.
- Tools — 11 tool files. Real implementations: `memory_save`, `memory_recall`, `shell`, `file` (with path guards), `exec`, `set_status`. Stubs: `reply`, `branch_tool`, `spawn_worker`, `route`, `cancel`.
- `ToolServerHandle` wraps Rig's `ToolSet` with `register()`, but individual tools don't implement Rig's `Tool` trait yet — they're standalone async functions.
- Core types in `lib.rs` — `InboundMessage`, `OutboundResponse`, `StatusUpdate`, `ProcessEvent`, `AgentDeps`, `ProcessId`, `ProcessType`, `ChannelId`, `WorkerId`, `BranchId`
- `main.rs` — CLI (clap), tracing, config/DB/LLM/memory init (constructs `MemorySearch` with all components), event loop, graceful shutdown

**What's missing:**
- Tools not registered as Rig `Tool` trait impls (no `const NAME`, no `Args`/`Output` types, no `JsonSchema`). Current tools are standalone `async fn`s that take `AgentDeps` as a parameter — fundamentally different shape from Rig's `Tool::call(&self, args)`. Each tool will need to hold its dependencies (MemorySearch, event channels, etc.) as struct fields.
- `SpacebotHook` has standalone methods but doesn't implement Rig's `PromptHook<SpacebotModel>` trait — not wired into the agent loop
- `ToolServerHandle` has a broken `Clone` impl (creates empty `ToolSet`, losing all registered tools)
- No identity files (SOUL.md, IDENTITY.md, USER.md)
- Agent LLM calls are simulated (placeholder `tokio::time::sleep` instead of real `agent.prompt()`)
- Streaming not implemented (SpacebotModel.stream() returns error)
- Secrets and settings stores are empty stubs

**Known issues:**
- `embedding.rs` `embed_one()` async path creates a new fastembed model per call instead of sharing via Arc (the sync `embed_one_blocking()` works correctly and is what `hybrid_search` uses)
- Arrow version mismatch in Cargo.toml: `arrow = "54"` vs `arrow-array`/`arrow-schema` at `"57.3.0"` — should align or drop the `arrow` meta-crate
- `lance.rs` casts `_distance`/`_score` columns as `Float64Type` — LanceDB may return `Float32`, risking a runtime panic on cast

---

## ~~Phase 1: Migrations and LanceDB~~ Done

- [x] SQLite migrations for all tables (memories, associations, conversations, heartbeats)
- [x] Inline DDL removed from `memory/store.rs`, `conversation/history.rs`, `heartbeat/store.rs`
- [x] `memory/lance.rs` — LanceDB table with Arrow schema, embedding insert, vector search (cosine), FTS (Tantivy), index creation
- [x] Embedding generation wired into memory save flow (`memory_save.rs` generates + stores)
- [x] Vector + FTS results connected into hybrid search via `MemorySearch` struct
- [x] `MemorySearch` bundles `MemoryStore` + `EmbeddingTable` + `EmbeddingModel`, replaces `memory_store` in `AgentDeps`

---

## Phase 2: Wire Tools to Rig

Individual tools need to implement Rig's `Tool` trait so they work with `AgentBuilder.tool()` and the agentic loop. The current tools are standalone `async fn`s that take `AgentDeps` — they need to become structs that hold their dependencies and implement `Tool::call(&self, args)`.

- [ ] Reshape tools as structs with dependency fields (e.g., `MemorySaveTool { memory_search: Arc<MemorySearch>, event_tx: mpsc::Sender<ProcessEvent> }`)
- [ ] Implement Rig's `Tool` trait on each struct (`const NAME`, `Args: Deserialize + JsonSchema`, `Output: Serialize`, `definition()`, `call()`)
- [ ] Create shared ToolServer for channel/branch tools (reply, branch, spawn_worker, memory_save, route, cancel)
- [ ] Create per-worker ToolServer factory for task tools (shell, file, exec, set_status)
- [ ] Delete the custom `ToolServerHandle` wrapper — use Rig's `ToolServer::run()` → `ToolServerHandle` directly (channel-based, Clone works correctly)
- [ ] Update `AgentDeps` to hold a real `rig::tool::server::ToolServerHandle`
- [ ] Implement `PromptHook<SpacebotModel>` on `SpacebotHook` — wire the existing standalone methods into Rig's trait (on_tool_call, on_tool_result, on_completion_response, etc.)
- [ ] Implement `PromptHook<SpacebotModel>` on `CortexHook`

**Reference:** Rig's `Tool` trait: `const NAME`, `type Args`, `type Output`, `fn definition()`, `fn call()`. Doc comments on input structs serve as LLM instructions. `ToolServer::run()` consumes the server and returns a handle (channel-based, Clone is free). See `docs/research/rig-integration.md` for full `PromptHook` method signatures and `ToolCallHookAction` variants.

---

## ~~Phase 3: System Prompts and Identity~~ Done

- [x] `prompts/` directory with all 5 prompt files (CHANNEL.md, BRANCH.md, WORKER.md, COMPACTOR.md, CORTEX.md)
- [x] `identity/files.rs` — `Prompts` struct, `load_all_prompts()`, per-type loaders
- [x] `conversation/context.rs` — `build_channel_context()` (prompt + status + identity memories + high-importance memories), `build_branch_context()`, `build_worker_context()`
- [x] `conversation/history.rs` — `HistoryStore` with save_turn, load_recent, compaction summaries

---

## Phase 4: Model Routing + The Channel (MVP Core)

Implement model routing so each process type uses the right model, then wire the channel as the first real agent.

- [ ] Implement `RoutingConfig` — process-type defaults, task-type overrides, fallback chains (see `docs/routing.md`)
- [ ] Add `resolve_for_process(process_type, task_type)` to `LlmManager`
- [ ] Implement fallback logic in `SpacebotModel` — retry with next model in chain on 429/502/503/504
- [ ] Rate limit tracking — deprioritize 429'd models for configurable cooldown
- [ ] Wire `AgentBuilder::new(model).preamble(&prompt).hook(spacebot_hook).tool_server_handle(tools).default_max_turns(5).build()`
- [ ] Replace placeholder message handling with `agent.prompt(&message).with_history(&mut history).max_turns(5).await`
- [ ] Wire status block injection — prepend rendered status to each prompt call
- [ ] Connect conversation history persistence (HistoryStore already implemented) to channel message flow
- [ ] Fire-and-forget DB writes for message persistence (`tokio::spawn`, don't block the response)
- [ ] Test: send a message to a channel, get a real LLM response back

**Reference:** `docs/routing.md` for the full routing design. Rig's `agent.prompt().with_history(&mut history).max_turns(5)` is the core call. The channel never blocks on branches, workers, or compaction.

---

## Phase 5: Branches and Workers

Replace simulated branch/worker execution with real agent calls.

- [ ] Branch: wire `agent.prompt(&task).with_history(&mut branch_history).max_turns(10).await`
- [ ] Branch result injection — insert conclusion into channel history as a distinct message
- [ ] Branch concurrency limit enforcement (already scaffolded, needs testing)
- [ ] Worker: resolve model via `resolve_for_process(Worker, Some(task_type))`, wire `agent.prompt(&task).max_turns(50).await` with task-specific tools
- [ ] Interactive worker follow-ups — repeated `.prompt()` calls with accumulated history
- [ ] Worker status reporting via set_status tool → StatusBlock updates
- [ ] Handle stale branch results and worker timeout via Rig's `MaxTurnsError` / `PromptCancelled`

**Reference:** No existing codebase has context forking. Branch is `channel_history.clone()` run independently. Workers get fresh history + task description. Rig returns chat history in error types for recovery.

---

## Phase 6: Compactor

Wire the compaction workers to do real summarization.

- [ ] Implement compaction worker — summarize old turns + extract memories via LLM
- [ ] Emergency truncation — drop oldest turns without LLM, keep N recent
- [ ] Pre-compaction archiving — write raw transcript to conversation_archives table
- [ ] Non-blocking swap — replace old turns with summary while channel continues

**Reference:** IronClaw's tiered compaction (80/85/95 thresholds, already implemented). The novel part is the non-blocking swap.

---

## Phase 7: Webhook Messaging Adapter

Get a real end-to-end messaging path working.

- [ ] Implement WebhookAdapter (axum) — POST endpoint, InboundMessage production, response routing
- [ ] Implement MessagingManager.start() — spawn adapters, merge inbound streams via `select_all`
- [ ] Implement outbound routing — responses flow from channel → manager → correct adapter
- [ ] Optional sync mode (`"wait": true` blocks until agent responds)
- [ ] Wire the full path: HTTP POST → InboundMessage → Channel → response → OutboundResponse → HTTP response
- [ ] Test: curl a message in, get a response back

**Reference:** IronClaw's Channel trait and ChannelManager with `futures::stream::select_all()`. The Messaging trait and MessagingDyn companion are already implemented.

---

## Phase 8: End-to-End Integration

Wire everything together into a running system.

- [ ] main.rs orchestration — init config, DB, LLM, memory, tools, messaging, start event loop
- [ ] Event routing — ProcessEvent fan-in from all agents, dispatch to appropriate handlers
- [ ] Channel lifecycle — create on first message, persist across restarts, resume from DB
- [ ] Test the full loop: message in → channel → branch → worker → memory save → response out
- [ ] Graceful shutdown — broadcast signal, drain in-flight work, close DB connections

---

## Post-MVP

Not blocking the first working version, but next in line.

- **Streaming** — implement `SpacebotModel.stream()` with SSE parsing, wire through messaging adapters with block coalescing (see `docs/messaging.md`)
- **Cortex** — system-level observer, memory consolidation, decay management. No reference codebase for this.
- **Heartbeats** — scheduled tasks with fresh channels. Circuit breaker (3 failures → disable).
- **Telegram adapter** — real messaging platform integration.
- **Discord adapter** — thread-based conversations map naturally to channels.
- **Secrets store** — AES-256-GCM encrypted credentials in redb.
- **Settings store** — redb key-value with env > DB > default resolution.
- **Memory graph traversal during recall** — walk typed edges (Updates, Contradicts, CausedBy) during search.
- **Multi-channel identity coherence** — same soul across conversations, cortex consolidates across channels.
