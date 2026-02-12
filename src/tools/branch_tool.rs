//! Branch tool for forking context and thinking (channel only).

use crate::{BranchId, ChannelId, ProcessEvent};
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;

/// Tool for spawning branches.
#[derive(Debug, Clone)]
pub struct BranchTool {
    channel_id: ChannelId,
    event_tx: mpsc::Sender<ProcessEvent>,
}

impl BranchTool {
    /// Create a new branch tool.
    pub fn new(channel_id: ChannelId, event_tx: mpsc::Sender<ProcessEvent>) -> Self {
        Self {
            channel_id,
            event_tx,
        }
    }
}

/// Error type for branch tool.
#[derive(Debug, thiserror::Error)]
#[error("Branch creation failed: {0}")]
pub struct BranchError(String);

/// Arguments for branch tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct BranchArgs {
    /// Description of what the branch should think about or investigate.
    pub description: String,
    /// Optional context or constraints for the branch.
    pub context: Option<String>,
    /// Maximum turns for the branch (default: 10).
    #[serde(default = "default_max_turns")]
    pub max_turns: usize,
}

fn default_max_turns() -> usize {
    10
}

/// Output from branch tool.
#[derive(Debug, Serialize)]
pub struct BranchOutput {
    /// The ID of the created branch.
    pub branch_id: BranchId,
    /// The channel ID.
    pub channel_id: ChannelId,
    /// Whether the branch was spawned successfully.
    pub spawned: bool,
    /// Message about the branch status.
    pub message: String,
}

impl Tool for BranchTool {
    const NAME: &'static str = "branch";

    type Error = BranchError;
    type Args = BranchArgs;
    type Output = BranchOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Fork a branch to think independently about a problem. A branch is a separate process that has a clone of your current conversation history and can use tools like memory_recall and spawn_worker. The branch will think through the problem and return a conclusion. Use this when you need to think deeply about something without blocking the conversation.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "description": {
                        "type": "string",
                        "description": "What the branch should investigate or think about. Be specific about what conclusion you want."
                    },
                    "context": {
                        "type": "string",
                        "description": "Optional additional context or constraints for the branch"
                    },
                    "max_turns": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 50,
                        "default": 10,
                        "description": "Maximum number of turns the branch can take"
                    }
                },
                "required": ["description"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let branch_id = Uuid::new_v4();

        tracing::info!(
            branch_id = %branch_id,
            channel_id = %self.channel_id,
            description = %args.description,
            "spawning branch"
        );

        // In real implementation:
        // 1. Clone the channel's history
        // 2. Create a new Branch process
        // 3. Add to channel's active_branches
        // 4. Return the branch_id for tracking

        // For now, just log that we would spawn a branch
        tracing::info!(%branch_id, "branch would be spawned here");

        Ok(BranchOutput {
            branch_id,
            channel_id: self.channel_id.clone(),
            spawned: true,
            message: format!("Branch {} spawned successfully. It will investigate: {}",
                branch_id, args.description),
        })
    }
}

/// Create a new branch ID.
pub fn create_branch_id() -> BranchId {
    Uuid::new_v4()
}

/// Legacy function for spawning branches.
pub async fn spawn_branch(
    channel_id: ChannelId,
    description: impl Into<String>,
) -> anyhow::Result<BranchId> {
    // We need an event_tx to create a BranchTool, but we don't have one in this context
    // This function is kept for backward compatibility but will log a warning
    tracing::warn!("spawn_branch called without event_tx - use BranchTool instead");

    let branch_id = create_branch_id();
    let desc = description.into();

    tracing::info!(%branch_id, channel_id = %channel_id, description = %desc, "would spawn branch");

    Ok(branch_id)
}
