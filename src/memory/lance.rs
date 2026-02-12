//! LanceDB table management and embedding storage with HNSW vector index and FTS.

use crate::error::{DbError, Result};
use arrow_array::{Array, RecordBatchIterator};
use arrow_array::cast::AsArray;
use arrow_array::types::{Float64Type, Float32Type};
use futures::TryStreamExt;
use std::sync::Arc;

/// Schema constants for the embeddings table.
const TABLE_NAME: &str = "memory_embeddings";
const EMBEDDING_DIM: i32 = 384; // all-MiniLM-L6-v2 dimension

/// LanceDB table for memory embeddings with HNSW index and FTS.
pub struct EmbeddingTable {
    table: lancedb::Table,
}

impl EmbeddingTable {
    /// Open existing table or create a new one.
    pub async fn open_or_create(connection: &lancedb::Connection) -> Result<Self> {
        // Try to open existing table first
        match connection.open_table(TABLE_NAME).execute().await {
            Ok(table) => Ok(Self { table }),
            Err(_) => {
                // Create new table with empty batch
                let schema = Self::schema();
                
                // Create empty RecordBatchIterator
                let batches = RecordBatchIterator::new(
                    vec![].into_iter().map(Ok),
                    Arc::new(schema),
                );
                
                let table = connection
                    .create_table(TABLE_NAME, Box::new(batches))
                    .execute()
                    .await
                    .map_err(|e| DbError::LanceDb(e.to_string()))?;
                
                Ok(Self { table })
            }
        }
    }
    
    /// Store an embedding with content for a memory.
    /// The content is stored for FTS search capability.
    pub async fn store(
        &self,
        memory_id: &str,
        content: &str,
        embedding: &[f32],
    ) -> Result<()> {
        if embedding.len() != EMBEDDING_DIM as usize {
            return Err(DbError::LanceDb(format!(
                "Embedding dimension mismatch: expected {}, got {}",
                EMBEDDING_DIM,
                embedding.len()
            )).into());
        }
        
        use arrow_array::{FixedSizeListArray, RecordBatch, StringArray};
        use arrow_array::types::Float32Type;
        
        let schema = Self::schema();
        
        // Build arrays for the record batch
        let id_array = StringArray::from(vec![memory_id]);
        let content_array = StringArray::from(vec![content]);
        
        // Convert embedding to FixedSizeListArray
        let embedding_array = arrow_array::FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            vec![Some(embedding.iter().map(|v| Some(*v)).collect::<Vec<_>>())],
            EMBEDDING_DIM,
        );
        
        let batch = RecordBatch::try_new(
            Arc::new(schema),
            vec![
                Arc::new(id_array) as arrow_array::ArrayRef,
                Arc::new(content_array) as arrow_array::ArrayRef,
                Arc::new(embedding_array) as arrow_array::ArrayRef,
            ],
        )
        .map_err(|e| DbError::LanceDb(e.to_string()))?;
        
        // Create iterator for IntoArrow trait
        let batches = RecordBatchIterator::new(
            vec![Ok(batch)],
            Arc::new(Self::schema()),
        );
        
        self.table
            .add(Box::new(batches))
            .execute()
            .await
            .map_err(|e| DbError::LanceDb(e.to_string()))?;
        
        Ok(())
    }
    
    /// Delete an embedding by memory ID.
    pub async fn delete(&self, memory_id: &str) -> Result<()> {
        let predicate = format!("id = '{}'", memory_id);
        self.table
            .delete(&predicate)
            .await
            .map_err(|e| DbError::LanceDb(e.to_string()))?;
        
        Ok(())
    }
    
    /// Vector similarity search using cosine distance.
    /// Returns (memory_id, distance) pairs sorted by distance (ascending).
    pub async fn vector_search(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(String, f32)>> {
        if query_embedding.len() != EMBEDDING_DIM as usize {
            return Err(DbError::LanceDb(format!(
                "Query embedding dimension mismatch: expected {}, got {}",
                EMBEDDING_DIM,
                query_embedding.len()
            )).into());
        }
        
        use lancedb::query::{ExecutableQuery, QueryBase};
        
        // Use query() API with nearest_to for vector search
        let results: Vec<arrow_array::RecordBatch> = self
            .table
            .query()
            .nearest_to(query_embedding)
            .map_err(|e| DbError::LanceDb(e.to_string()))?
            .limit(limit)
            .execute()
            .await
            .map_err(|e| DbError::LanceDb(e.to_string()))?
            .try_collect()
            .await
            .map_err(|e| DbError::LanceDb(e.to_string()))?;
        
        let mut matches = Vec::new();
        for batch in results {
            if let (Some(id_col), Some(dist_col)) = (batch.column_by_name("id"), batch.column_by_name("_distance")) {
                let ids: &arrow_array::StringArray = id_col.as_string::<i32>();
                let dists: &arrow_array::PrimitiveArray<Float64Type> = dist_col.as_primitive();
                
                for i in 0..ids.len() {
                    if ids.is_valid(i) && dists.is_valid(i) {
                        let id = ids.value(i).to_string();
                        let distance = dists.value(i) as f32;
                        matches.push((id, distance));
                    }
                }
            }
        }
        
        Ok(matches)
    }
    
    /// Full-text search using Tantivy FTS.
    /// Returns (memory_id, score) pairs sorted by score (descending).
    pub async fn text_search(&self, query: &str, limit: usize) -> Result<Vec<(String, f32)>> {
        use lancedb::query::{ExecutableQuery, QueryBase};
        
        // Use full_text_search on the content column
        let results: Vec<arrow_array::RecordBatch> = self
            .table
            .query()
            .full_text_search(lance_index::scalar::FullTextSearchQuery::new(query.to_string()))
            .select(lancedb::query::Select::columns(&["id", "_score"]))
            .limit(limit)
            .execute()
            .await
            .map_err(|e| DbError::LanceDb(e.to_string()))?
            .try_collect()
            .await
            .map_err(|e| DbError::LanceDb(e.to_string()))?;
        
        let mut matches = Vec::new();
        for batch in results {
            if let (Some(id_col), Some(score_col)) = (batch.column_by_name("id"), batch.column_by_name("_score")) {
                let ids: &arrow_array::StringArray = id_col.as_string::<i32>();
                let scores: &arrow_array::PrimitiveArray<Float64Type> = score_col.as_primitive();
                
                for i in 0..ids.len() {
                    if ids.is_valid(i) && scores.is_valid(i) {
                        let id = ids.value(i).to_string();
                        let score = scores.value(i) as f32;
                        matches.push((id, score));
                    }
                }
            }
        }
        
        Ok(matches)
    }
    
    /// Create HNSW vector index and FTS index for better performance.
    /// Should be called after enough data accumulates.
    pub async fn create_indexes(&self) -> Result<()> {
        // Create HNSW vector index on embedding column
        self.table
            .create_index(&["embedding"], lancedb::index::Index::Auto)
            .execute()
            .await
            .map_err(|e| DbError::LanceDb(format!("Failed to create vector index: {}", e)))?;
        
        // Create FTS index on content column using default FTS options
        self.table
            .create_index(&["content"], lancedb::index::Index::FTS(Default::default()))
            .execute()
            .await
            .map_err(|e| DbError::LanceDb(format!("Failed to create FTS index: {}", e)))?;
        
        Ok(())
    }
    
    /// Get the Arrow schema for the embeddings table.
    fn schema() -> arrow_schema::Schema {
        arrow_schema::Schema::new(vec![
            arrow_schema::Field::new("id", arrow_schema::DataType::Utf8, false),
            arrow_schema::Field::new("content", arrow_schema::DataType::Utf8, false),
            arrow_schema::Field::new(
                "embedding",
                arrow_schema::DataType::FixedSizeList(
                    Arc::new(arrow_schema::Field::new("item", arrow_schema::DataType::Float32, true)),
                    EMBEDDING_DIM,
                ),
                false,
            ),
        ])
    }
}
