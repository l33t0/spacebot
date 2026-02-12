//! Memory save tool for channels and branches.

use crate::error::Result;
use crate::memory::{Memory, MemorySearch, MemoryType};
use crate::memory::types::{CreateMemoryInput, Association, RelationType};
use std::sync::Arc;

/// Save a memory to the store.
/// Generates and stores an embedding after saving to SQLite.
pub async fn memory_save(
    memory_search: &MemorySearch,
    input: CreateMemoryInput,
) -> Result<String> {
    // Create the memory
    let mut memory = Memory::new(&input.content, input.memory_type);
    
    if let Some(importance) = input.importance {
        memory = memory.with_importance(importance);
    }
    
    if let Some(source) = input.source {
        memory = memory.with_source(source);
    }
    
    if let Some(channel_id) = input.channel_id {
        memory = memory.with_channel_id(channel_id);
    }
    
    // Save to SQLite database
    let store = memory_search.store();
    store.save(&memory).await?;
    
    // Create associations
    for assoc_input in input.associations {
        let association = Association::new(
            &memory.id,
            &assoc_input.target_id,
            assoc_input.relation_type,
        ).with_weight(assoc_input.weight);
        
        store.create_association(&association).await?;
    }
    
    // Store embedding in LanceDB
    let embedding = if let Some(embedding) = input.embedding {
        // Use provided embedding
        embedding
    } else {
        // Generate embedding using the shared model
        memory_search.embedding_model().embed_one_blocking(&input.content)?
    };
    
    memory_search.embedding_table().store(&memory.id, &input.content, &embedding).await?;
    
    Ok(memory.id)
}

/// Convenience function for simple fact saving.
pub async fn save_fact(
    memory_search: &MemorySearch,
    content: impl Into<String>,
    channel_id: Option<crate::ChannelId>,
) -> Result<String> {
    let input = CreateMemoryInput {
        content: content.into(),
        memory_type: MemoryType::Fact,
        importance: None,
        source: None,
        channel_id,
        embedding: None,
        associations: vec![],
    };
    
    memory_save(memory_search, input).await
}
