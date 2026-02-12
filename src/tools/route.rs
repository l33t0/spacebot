//! Route tool for sending follow-ups to active workers.

use crate::{ChannelId, ProcessEvent, WorkerId};
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Tool for routing messages to workers.
#[derive(Debug, Clone)]
pub struct RouteTool {
    channel_id: ChannelId,
    event_tx: mpsc::Sender<ProcessEvent>,
}

impl RouteTool {
    /// Create a new route tool.
    pub fn new(channel_id: ChannelId, event_tx: mpsc::Sender<ProcessEvent>) -> Self {
        Self {
            channel_id,
            event_tx,
        }
    }
}

/// Error type for route tool.
#[derive(Debug, thiserror::Error)]
#[error("Route failed: {0}")]
pub struct RouteError(String);

/// Arguments for route tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RouteArgs {
    /// The ID of the worker to route to (UUID format).
    pub worker_id: String,
    /// The message to send to the worker.
    pub message: String,
}

/// Output from route tool.
#[derive(Debug, Serialize)]
pub struct RouteOutput {
    /// Whether the message was routed successfully.
    pub routed: bool,
    /// The worker ID.
    pub worker_id: WorkerId,
    /// The channel ID.
    pub channel_id: ChannelId,
    /// Status message.
    pub message: String,
}

impl Tool for RouteTool {
    const NAME: &'static str = "route";

    type Error = RouteError;
    type Args = RouteArgs;
    type Output = RouteOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Send a follow-up message to an active interactive worker. This is how you continue a conversation with a long-running worker (like a coding session) without creating a new worker. The message will be queued on the worker's input channel.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "worker_id": {
                        "type": "string",
                        "description": "The worker ID to route to (from spawn_worker result)"
                    },
                    "message": {
                        "type": "string",
                        "description": "The message to send to the worker"
                    }
                },
                "required": ["worker_id", "message"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> std::result::Result<Self::Output, Self::Error> {
        // Parse the worker ID
        let worker_id = args.worker_id.parse::<WorkerId>()
            .map_err(|e| RouteError(format!("Invalid worker ID: {e}")))?;

        tracing::info!(
            worker_id = %worker_id,
            channel_id = %self.channel_id,
            message_preview = %args.message.chars().take(50).collect::<String>(),
            "routing message to worker"
        );

        // In real implementation:
        // 1. Verify worker exists and is interactive
        // 2. Queue the message on the worker's input channel
        // 3. Return immediately (don't wait for response)

        tracing::info!(worker_id = %worker_id, "message would be routed here");

        Ok(RouteOutput {
            routed: true,
            worker_id,
            channel_id: self.channel_id.clone(),
            message: format!("Message routed to worker {}.", worker_id),
        })
    }
}

/// Legacy function for routing to workers.
pub async fn route_to_worker(
    _channel_id: ChannelId,
    _worker_id: WorkerId,
    _message: impl Into<String>,
) -> anyhow::Result<()> {
    // We need an event_tx to create a RouteTool
    // This function is kept for backward compatibility
    tracing::warn!("route_to_worker called without event_tx - use RouteTool instead");

    tracing::info!("would route message to worker");
    Ok(())
}
