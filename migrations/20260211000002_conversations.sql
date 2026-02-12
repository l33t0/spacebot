-- Conversation turns table (raw message history)
CREATE TABLE IF NOT EXISTS conversation_turns (
    id TEXT PRIMARY KEY,
    channel_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    inbound_message TEXT NOT NULL,
    outbound_response TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(channel_id, sequence)
);

-- Compaction summaries table (replaces archived conversation turns)
CREATE TABLE IF NOT EXISTS compaction_summaries (
    id TEXT PRIMARY KEY,
    channel_id TEXT NOT NULL,
    start_sequence INTEGER NOT NULL,
    end_sequence INTEGER NOT NULL,
    summary_text TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for conversation queries
CREATE INDEX IF NOT EXISTS idx_turns_channel ON conversation_turns(channel_id);
CREATE INDEX IF NOT EXISTS idx_turns_sequence ON conversation_turns(channel_id, sequence);
