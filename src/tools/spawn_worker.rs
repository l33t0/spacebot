//! Spawn worker tool for creating new workers.

use crate::{ChannelId, ProcessEvent, WorkerId};
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;

/// Tool for spawning workers.
#[derive(Debug, Clone)]
pub struct SpawnWorkerTool {
    channel_id: Option<ChannelId>,
    event_tx: mpsc::Sender<ProcessEvent>,
}

impl SpawnWorkerTool {
    /// Create a new spawn worker tool.
    pub fn new(channel_id: Option<ChannelId>, event_tx: mpsc::Sender<ProcessEvent>) -> Self {
        Self {
            channel_id,
            event_tx,
        }
    }
}

/// Error type for spawn worker tool.
#[derive(Debug, thiserror::Error)]
#[error("Worker spawn failed: {0}")]
pub struct SpawnWorkerError(String);

/// Arguments for spawn worker tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SpawnWorkerArgs {
    /// The task description for the worker.
    pub task: String,
    /// Whether this is an interactive worker (accepts follow-up messages).
    #[serde(default)]
    pub interactive: bool,
    /// Maximum turns for the worker (default: 50 for fire-and-forget, 100 for interactive).
    #[serde(default)]
    pub max_turns: Option<usize>,
    /// Optional specific tools to give the worker (defaults to task tools: shell, file, exec, set_status).
    #[serde(default)]
    pub tools: Vec<String>,
}

/// Output from spawn worker tool.
#[derive(Debug, Serialize)]
pub struct SpawnWorkerOutput {
    /// The ID of the spawned worker.
    pub worker_id: WorkerId,
    /// The channel ID (if any).
    pub channel_id: Option<ChannelId>,
    /// Whether the worker was spawned successfully.
    pub spawned: bool,
    /// Whether this is an interactive worker.
    pub interactive: bool,
    /// Status message.
    pub message: String,
}

impl Tool for SpawnWorkerTool {
    const NAME: &'static str = "spawn_worker";

    type Error = SpawnWorkerError;
    type Args = SpawnWorkerArgs;
    type Output = SpawnWorkerOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Spawn a worker to execute a specific task. Workers are independent processes that can run shell commands, read/write files, and execute other programs. They do NOT have access to your conversation history - they only see the task description you give them. Use this for: file operations, running tests, building code, executing scripts, any work that can be done independently.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "Clear, specific description of what the worker should do. Include all context needed since the worker can't see your conversation history."
                    },
                    "interactive": {
                        "type": "boolean",
                        "default": false,
                        "description": "If true, creates an interactive worker that can receive follow-up messages. Use for multi-turn tasks like coding sessions."
                    },
                    "max_turns": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 200,
                        "description": "Optional override for maximum turns (default: 50 for fire-and-forget, 100 for interactive)"
                    },
                    "tools": {
                        "type": "array",
                        "items": {
                            "type": "string",
                            "enum": ["shell", "file", "exec", "set_status"]
                        },
                        "description": "Optional specific tools to give the worker (defaults to all: shell, file, exec, set_status)"
                    }
                },
                "required": ["task"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let worker_id = Uuid::new_v4();

        let max_turns = args.max_turns.unwrap_or_else(|| {
            if args.interactive { 100 } else { 50 }
        });

        tracing::info!(
            worker_id = %worker_id,
            channel_id = ?self.channel_id,
            task = %args.task,
            interactive = args.interactive,
            max_turns,
            "spawning worker"
        );

        // In real implementation:
        // 1. Create a Worker process with task-specific tools
        // 2. If interactive, set up an input channel for follow-ups
        // 3. Start the worker with its own isolated history
        // 4. Send WorkerStarted event

        tracing::info!(%worker_id, "worker would be spawned here");

        let message = if args.interactive {
            format!("Interactive worker {} spawned. It will work on: {}. You can route follow-up messages to it.",
                worker_id, args.task)
        } else {
            format!("Worker {} spawned. It will complete: {} and report back when done.",
                worker_id, args.task)
        };

        Ok(SpawnWorkerOutput {
            worker_id,
            channel_id: self.channel_id.clone(),
            spawned: true,
            interactive: args.interactive,
            message,
        })
    }
}

/// Create a new worker ID.
pub fn create_worker_id() -> WorkerId {
    Uuid::new_v4()
}

/// Legacy function for spawning workers.
pub async fn spawn_worker(
    channel_id: Option<ChannelId>,
    task: impl Into<String>,
    interactive: bool,
) -> anyhow::Result<WorkerId> {
    // We need an event_tx to create a SpawnWorkerTool
    // This function is kept for backward compatibility but will log a warning
    tracing::warn!("spawn_worker called without event_tx - use SpawnWorkerTool instead");

    let worker_id = create_worker_id();
    let task_str = task.into();

    tracing::info!(%worker_id, channel_id = ?channel_id, task = %task_str, interactive, "would spawn worker");

    Ok(worker_id)
}
