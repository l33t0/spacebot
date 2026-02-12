# Discord Adapter Implementation Plan

Step-by-step plan for implementing `src/messaging/discord.rs`. Uses serenity `0.12.x`. Reference the local clone at `/Users/jamespine/Projects/serenity` for API details.

Before writing code, read `RUST_STYLE_GUIDE.md` and `AGENTS.md`.

For reference on how the messaging system works, read `docs/messaging.md`. For serenity API patterns, reference `serenity/examples/e01_basic_ping_bot` and `serenity/examples/e13_parallel_loops`.

## Dependencies

Add to `Cargo.toml`:

```toml
serenity = { version = "0.12", default-features = false, features = ["client", "gateway", "model", "cache", "rustls_backend"] }
```

We disable default features and pick what we need. `rustls_backend` avoids linking OpenSSL.

## The Struct

```rust
pub struct DiscordAdapter {
    token: String,
    guild_filter: Option<Vec<serenity::model::id::GuildId>>,
    http: Arc<RwLock<Option<Arc<serenity::http::Http>>>>,
    bot_user_id: Arc<RwLock<Option<serenity::model::id::UserId>>>,
    active_messages: Arc<RwLock<HashMap<String, serenity::model::id::MessageId>>>,
    typing_tasks: Arc<RwLock<HashMap<String, serenity::http::Typing>>>,
    shard_manager: Arc<RwLock<Option<Arc<serenity::gateway::ShardManager>>>>,
}
```

- `http` is `None` until `start()` builds the client and clones `client.http`
- `bot_user_id` is set from the `Ready` event
- `active_messages` tracks streaming responses: maps `InboundMessage.id` to the Discord `MessageId` being edited
- `typing_tasks` holds `Typing` handles per conversation. Serenity's `Typing` auto-repeats every 7s and stops on drop
- `shard_manager` for graceful shutdown

## The EventHandler

A separate struct that holds an `mpsc::Sender<InboundMessage>` and the shared state refs needed to populate it.

```rust
struct Handler {
    inbound_tx: mpsc::Sender<InboundMessage>,
    guild_filter: Option<Vec<GuildId>>,
    http_slot: Arc<RwLock<Option<Arc<Http>>>>,
    bot_user_id_slot: Arc<RwLock<Option<UserId>>>,
    shard_manager_slot: Arc<RwLock<Option<Arc<ShardManager>>>>,
}
```

### `ready()` implementation

```rust
async fn ready(&self, ctx: Context, ready: Ready) {
    tracing::info!(bot_name = %ready.user.name, "discord connected");

    // Store the Http handle for use outside event handlers
    *self.http_slot.write().await = Some(ctx.http.clone());
    *self.bot_user_id_slot.write().await = Some(ready.user.id);

    // Log guilds
    tracing::info!(guild_count = ready.guilds.len(), "discord guilds available");
}
```

Note: `ctx.http` is `Arc<Http>`. We also need the shard manager — but it's on `Client`, not `Context`. Store it before calling `client.start()`.

### `message()` implementation

```rust
async fn message(&self, _ctx: Context, msg: Message) {
    // Ignore bots (including ourselves)
    if msg.author.bot {
        return;
    }

    // Guild filter
    if let Some(filter) = &self.guild_filter {
        if let Some(guild_id) = msg.guild_id {
            if !filter.contains(&guild_id) {
                return;
            }
        }
    }

    let conversation_id = build_conversation_id(&msg);
    let content = extract_content(&msg);
    let metadata = build_metadata(&msg);

    let inbound = InboundMessage {
        id: msg.id.to_string(),
        source: "discord".into(),
        conversation_id,
        sender_id: msg.author.id.to_string(),
        content,
        timestamp: msg.timestamp.to_utc(),
        metadata,
    };

    self.inbound_tx.send(inbound).await.ok();
}
```

## Helper Functions

### `build_conversation_id`

```rust
fn build_conversation_id(msg: &Message) -> String {
    match msg.guild_id {
        Some(guild_id) => format!("discord:{}:{}", guild_id, msg.channel_id),
        None => format!("discord:dm:{}", msg.author.id),
    }
}
```

Thread detection: messages in threads already have the thread's channel ID as `msg.channel_id`, so the conversation_id naturally scopes to the thread. No special handling needed — Discord treats threads as channels with their own IDs.

### `extract_content`

```rust
fn extract_content(msg: &Message) -> MessageContent {
    if msg.attachments.is_empty() {
        MessageContent::Text(msg.content.clone())
    } else {
        let attachments = msg.attachments.iter().map(|a| Attachment {
            filename: a.filename.clone(),
            mime_type: a.content_type.clone().unwrap_or_default(),
            url: a.url.clone(),
            size_bytes: Some(a.size as u64),
        }).collect();

        MessageContent::Media {
            text: if msg.content.is_empty() { None } else { Some(msg.content.clone()) },
            attachments,
        }
    }
}
```

### `build_metadata`

Store everything needed to route responses back:

```rust
fn build_metadata(msg: &Message) -> HashMap<String, serde_json::Value> {
    let mut metadata = HashMap::new();
    metadata.insert("discord_channel_id".into(), msg.channel_id.get().into());
    metadata.insert("discord_message_id".into(), msg.id.get().into());
    metadata.insert("discord_author_name".into(), msg.author.name.clone().into());

    if let Some(guild_id) = msg.guild_id {
        metadata.insert("discord_guild_id".into(), guild_id.get().into());
    }

    metadata
}
```

Use `.get()` on ID types — they're newtypes over `u64` and `.get()` returns the inner value which serializes cleanly.

## Implementing the `Messaging` Trait

### `name()`

```rust
fn name(&self) -> &str { "discord" }
```

### `start()`

```rust
async fn start(&self) -> Result<InboundStream> {
    let (inbound_tx, inbound_rx) = mpsc::channel(256);

    let handler = Handler {
        inbound_tx,
        guild_filter: self.guild_filter.clone(),
        http_slot: self.http.clone(),
        bot_user_id_slot: self.bot_user_id.clone(),
        shard_manager_slot: self.shard_manager.clone(),
    };

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILDS;

    let mut client = serenity::Client::builder(&self.token, intents)
        .event_handler(handler)
        .await
        .context("failed to build discord client")?;

    // Extract Http and ShardManager before start() consumes the client
    *self.http.write().await = Some(client.http.clone());
    *self.shard_manager.write().await = Some(client.shard_manager.clone());

    // Spawn the gateway connection — client.start() blocks
    tokio::spawn(async move {
        if let Err(error) = client.start().await {
            tracing::error!(%error, "discord gateway error");
        }
    });

    // Convert mpsc::Receiver to a Stream
    let stream = tokio_stream::wrappers::ReceiverStream::new(inbound_rx);
    Ok(Box::pin(stream))
}
```

Note: we set `self.http` both here (from `client.http`) and in the `ready()` handler. The one from `client.http` is available immediately. The one from `ready()` is just confirmation that the gateway is connected. Both are the same `Arc<Http>`.

### `respond()`

```rust
async fn respond(&self, message: &InboundMessage, response: OutboundResponse) -> Result<()> {
    let http = self.get_http().await?;
    let channel_id = self.extract_channel_id(message)?;

    match response {
        OutboundResponse::Text(text) => {
            // Stop any active typing
            self.stop_typing(&message.id).await;

            // Split long messages at 2000 chars
            for chunk in split_message(&text, 2000) {
                channel_id.say(&*http, &chunk).await
                    .context("failed to send discord message")?;
            }
        }
        OutboundResponse::StreamStart => {
            // Stop typing, send placeholder
            self.stop_typing(&message.id).await;

            let placeholder = channel_id.say(&*http, "\u{200B}") // zero-width space
                .await
                .context("failed to send stream placeholder")?;

            self.active_messages.write().await
                .insert(message.id.clone(), placeholder.id);
        }
        OutboundResponse::StreamChunk(text) => {
            let active = self.active_messages.read().await;
            if let Some(&msg_id) = active.get(&message.id) {
                // Truncate to 2000 chars for the edit
                let display_text = if text.len() > 2000 {
                    format!("{}...", &text[..1997])
                } else {
                    text
                };
                let builder = EditMessage::new().content(display_text);
                if let Err(error) = channel_id.edit_message(&*http, msg_id, builder).await {
                    tracing::warn!(%error, "failed to edit streaming message");
                }
            }
        }
        OutboundResponse::StreamEnd => {
            self.active_messages.write().await.remove(&message.id);
        }
    }
    Ok(())
}
```

### `send_status()`

```rust
async fn send_status(&self, message: &InboundMessage, status: StatusUpdate) -> Result<()> {
    match status {
        StatusUpdate::Thinking => {
            let http = self.get_http().await?;
            let channel_id = self.extract_channel_id(message)?;

            // Serenity's Typing auto-repeats every 7s and stops on drop
            let typing = channel_id.start_typing(&http);
            self.typing_tasks.write().await
                .insert(message.id.clone(), typing);
        }
        _ => {
            // Any non-Thinking status stops typing
            self.stop_typing(&message.id).await;
        }
    }
    Ok(())
}
```

### `broadcast()`

For proactive messages (heartbeat results, worker completions with notify flag):

```rust
async fn broadcast(&self, target: &str, response: OutboundResponse) -> Result<()> {
    let http = self.get_http().await?;

    // target is a Discord channel ID as a string
    let channel_id = ChannelId::new(
        target.parse::<u64>().context("invalid discord channel id")?
    );

    if let OutboundResponse::Text(text) = response {
        for chunk in split_message(&text, 2000) {
            channel_id.say(&*http, &chunk).await
                .context("failed to broadcast discord message")?;
        }
    }

    Ok(())
}
```

### `health_check()`

```rust
async fn health_check(&self) -> Result<()> {
    let http = self.get_http().await?;
    http.get_current_user().await
        .context("discord health check failed")?;
    Ok(())
}
```

### `shutdown()`

```rust
async fn shutdown(&self) -> Result<()> {
    // Stop all typing indicators
    self.typing_tasks.write().await.clear();

    // Shut down the gateway connection
    if let Some(shard_manager) = self.shard_manager.read().await.as_ref() {
        shard_manager.shutdown_all().await;
    }

    tracing::info!("discord adapter shut down");
    Ok(())
}
```

## Private Helper Methods

```rust
impl DiscordAdapter {
    /// Get the Http handle, failing if not connected yet.
    async fn get_http(&self) -> Result<Arc<Http>> {
        self.http.read().await
            .clone()
            .context("discord not connected")
    }

    /// Extract the Discord channel ID from message metadata.
    fn extract_channel_id(&self, message: &InboundMessage) -> Result<ChannelId> {
        let id = message.metadata.get("discord_channel_id")
            .and_then(|v| v.as_u64())
            .context("missing discord_channel_id in metadata")?;
        Ok(ChannelId::new(id))
    }

    /// Stop the typing indicator for a message.
    async fn stop_typing(&self, message_id: &str) {
        // Typing stops when the handle is dropped
        self.typing_tasks.write().await.remove(message_id);
    }
}
```

## Message Splitting

Serenity has no built-in message chunking. Implement a simple splitter:

```rust
/// Split a message into chunks that fit within Discord's 2000 char limit.
/// Tries to split at newlines, then spaces, then hard-cuts.
fn split_message(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }

        // Try to split at a newline within the limit
        let split_at = remaining[..max_len]
            .rfind('\n')
            // Fall back to a space
            .or_else(|| remaining[..max_len].rfind(' '))
            // Hard cut as last resort
            .unwrap_or(max_len);

        chunks.push(remaining[..split_at].to_string());
        remaining = &remaining[split_at..].trim_start();
    }

    chunks
}
```

## Constructor

```rust
impl DiscordAdapter {
    pub fn new(token: impl Into<String>, guild_filter: Option<Vec<u64>>) -> Self {
        Self {
            token: token.into(),
            guild_filter: guild_filter.map(|ids| ids.into_iter().map(GuildId::new).collect()),
            http: Arc::new(RwLock::new(None)),
            bot_user_id: Arc::new(RwLock::new(None)),
            active_messages: Arc::new(RwLock::new(HashMap::new())),
            typing_tasks: Arc::new(RwLock::new(HashMap::new())),
            shard_manager: Arc::new(RwLock::new(None)),
        }
    }
}
```

## Stream Coalescing

The coalescer sits between the channel's streaming output and the adapter's `respond()` calls. It accumulates `StreamChunk` text and flushes edits based on thresholds.

This is NOT implemented in the adapter itself — it lives in the manager or the channel's output path. The adapter's `respond()` receives already-coalesced chunks. On the first pass, skip coalescing entirely and edit on every chunk. Add the coalescer once the basic flow works.

## Additional Dependencies

Add `tokio-stream` for converting the `mpsc::Receiver` into a `Stream`:

```toml
tokio-stream = "0.1"
```

## What to Implement

In order:

1. Add `serenity` and `tokio-stream` to `Cargo.toml`
2. Implement `DiscordAdapter` struct and `new()` constructor
3. Implement the `Handler` struct and `EventHandler` trait (`ready`, `message`)
4. Implement helper functions (`build_conversation_id`, `extract_content`, `build_metadata`, `split_message`)
5. Implement the `Messaging` trait on `DiscordAdapter` (`name`, `start`, `respond`, `send_status`, `broadcast`, `health_check`, `shutdown`)
6. Implement private helpers (`get_http`, `extract_channel_id`, `stop_typing`)
7. Add `use crate::messaging::discord::DiscordAdapter;` to `src/messaging.rs` re-exports if needed
8. Verify `cargo check` passes

## What NOT to Implement Yet

- Stream coalescing (edit on every chunk for now, add batching later)
- Thread auto-creation (just use existing channels/threads as conversations)
- Slash commands or interactions (message-based only)
- Reaction handling
- Embed formatting (plain text responses only)
- Voice
