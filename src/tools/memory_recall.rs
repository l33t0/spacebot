//! Memory recall tool for branches.

use crate::error::Result;
use crate::memory::{MemorySearch};
use crate::memory::search::{SearchConfig, curate_results};
use crate::memory::types::{Memory, MemorySearchResult};

/// Recall memories using hybrid search.
pub async fn memory_recall(
    memory_search: &MemorySearch,
    query: &str,
    max_results: usize,
) -> Result<Vec<Memory>> {
    // Perform hybrid search
    let config = SearchConfig {
        max_results_per_source: max_results * 2,
        ..Default::default()
    };
    
    let search_results = memory_search.hybrid_search(query, &config).await?;
    
    // Curate results to get the most relevant
    let curated = curate_results(&search_results, max_results);
    
    // Record access for found memories
    let store = memory_search.store();
    for memory in &curated {
        let _ = store.record_access(&memory.id).await;
    }
    
    Ok(curated.into_iter().cloned().collect())
}

/// Format memories for display to an agent.
pub fn format_memories(memories: &[Memory]) -> String {
    if memories.is_empty() {
        return "No relevant memories found.".to_string();
    }
    
    let mut output = String::from("## Relevant Memories\n\n");
    
    for (i, memory) in memories.iter().enumerate() {
        output.push_str(&format!(
            "{}. [{}] (importance: {:.2})\n   {}\n\n",
            i + 1,
            memory.memory_type,
            memory.importance,
            memory.content.lines().next().unwrap_or(&memory.content)
        ));
    }
    
    output
}
