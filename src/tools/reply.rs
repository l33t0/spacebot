//! Reply tool for sending messages to users (channel only).

use crate::{InboundMessage, OutboundResponse};
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Tool for replying to users.
#[derive(Debug, Clone)]
pub struct ReplyTool {
    message: Arc<InboundMessage>,
}

impl ReplyTool {
    /// Create a new reply tool.
    pub fn new(message: Arc<InboundMessage>) -> Self {
        Self { message }
    }
}

/// Error type for reply tool.
#[derive(Debug, thiserror::Error)]
#[error("Reply failed: {0}")]
pub struct ReplyError(String);

/// Arguments for reply tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReplyArgs {
    /// The message content to send to the user.
    pub content: String,
    /// Whether this is a streaming chunk (optional, for internal use).
    #[serde(default)]
    pub is_stream_chunk: bool,
}

/// Output from reply tool.
#[derive(Debug, Serialize)]
pub struct ReplyOutput {
    /// Whether the reply was sent successfully.
    pub success: bool,
    /// The conversation ID.
    pub conversation_id: String,
    /// The content that was sent.
    pub content: String,
}

impl Tool for ReplyTool {
    const NAME: &'static str = "reply";

    type Error = ReplyError;
    type Args = ReplyArgs;
    type Output = ReplyOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Send a reply to the user. This is how you respond to the user's message. Use this tool to provide answers, ask clarifying questions, or continue the conversation. The reply will be sent through the appropriate messaging platform.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The content to send to the user. Can be markdown formatted."
                    },
                    "is_stream_chunk": {
                        "type": "boolean",
                        "default": false,
                        "description": "Internal flag for streaming mode - usually leave as false"
                    }
                },
                "required": ["content"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let response = if args.is_stream_chunk {
            OutboundResponse::StreamChunk(args.content.clone())
        } else {
            OutboundResponse::Text(args.content.clone())
        };

        // In real implementation, this would route through MessagingManager
        // For now, just log it
        tracing::info!(
            conversation_id = %self.message.conversation_id,
            "sending reply to user"
        );

        let _ = response; // TODO: Route through messaging manager

        Ok(ReplyOutput {
            success: true,
            conversation_id: self.message.conversation_id.clone(),
            content: args.content,
        })
    }
}

/// Send a reply to a user message (legacy function).
pub async fn reply(
    message: &InboundMessage,
    content: impl Into<String>,
) -> anyhow::Result<()> {
    let tool = ReplyTool::new(Arc::new(message.clone()));
    let args = ReplyArgs {
        content: content.into(),
        is_stream_chunk: false,
    };

    let _output = tool.call(args).await.map_err(|e| anyhow::anyhow!(e))?;
    Ok(())
}

/// Send a streaming reply chunk (legacy function).
pub async fn reply_stream_chunk(
    message: &InboundMessage,
    chunk: impl Into<String>,
) -> anyhow::Result<()> {
    let tool = ReplyTool::new(Arc::new(message.clone()));
    let args = ReplyArgs {
        content: chunk.into(),
        is_stream_chunk: true,
    };

    let _output = tool.call(args).await.map_err(|e| anyhow::anyhow!(e))?;
    Ok(())
}
