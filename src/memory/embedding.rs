//! Embedding generation via fastembed.

use crate::error::{LlmError, Result};
use std::sync::Arc;

/// Embedding model wrapper with thread-safe sharing.
pub struct EmbeddingModel {
    model: fastembed::TextEmbedding,
}

impl EmbeddingModel {
    /// Create a new embedding model with the default all-MiniLM-L6-v2.
    pub fn new() -> Result<Self> {
        let model = fastembed::TextEmbedding::try_new(Default::default())
            .map_err(|e| LlmError::EmbeddingFailed(e.to_string()))?;
        
        Ok(Self { model })
    }
    
    /// Generate embeddings for multiple texts (blocking).
    pub fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        self.model.embed(texts, None)
            .map_err(|e| LlmError::EmbeddingFailed(e.to_string()).into())
    }
    
    /// Generate embedding for a single text (blocking).
    pub fn embed_one_blocking(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.embed(vec![text.to_string()])?;
        Ok(embeddings.into_iter().next().unwrap_or_default())
    }
    
    /// Generate embedding for a single text (async, spawns blocking task).
    /// Callers should share via Arc<EmbeddingModel> and clone Arc before calling.
    pub async fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let text = text.to_string();
        let result = tokio::task::spawn_blocking(move || {
            let model = fastembed::TextEmbedding::try_new(Default::default())
                .map_err(|e| crate::Error::Llm(crate::error::LlmError::EmbeddingFailed(e.to_string())))?;
            model.embed(vec![text], None)
                .map_err(|e| crate::Error::Llm(crate::error::LlmError::EmbeddingFailed(e.to_string())))
        })
        .await
        .map_err(|e| crate::Error::Other(anyhow::anyhow!("embedding task failed: {}", e)))??;
        
        Ok(result.into_iter().next().unwrap_or_default())
    }
}

impl Default for EmbeddingModel {
    fn default() -> Self {
        Self::new().expect("Failed to initialize embedding model")
    }
}

/// Async function to embed text using a shared model.
/// Prefer using Arc<EmbeddingModel>::embed_one() directly for better performance.
pub async fn embed_text(model: Arc<EmbeddingModel>, text: &str) -> Result<Vec<f32>> {
    model.embed_one(text).await
}
