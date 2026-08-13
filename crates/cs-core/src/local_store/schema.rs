//! SQLCipher schema definitions for the local store.
//!
//! Adapted from `chat-storage-search` §3.2, with extensions for
//! kdrive integration (node_id, version_id, storage_sink columns).

/// SQL statements to create the local store schema.
pub const SCHEMA_SQL: &str = r#"
-- Conversations
CREATE TABLE IF NOT EXISTS conversation (
    id              TEXT PRIMARY KEY,
    conversation_type TEXT NOT NULL DEFAULT 'direct',
    scope           TEXT NOT NULL DEFAULT 'b2c',
    tenant_id       TEXT,
    community_id    TEXT,
    domain_id       TEXT,
    name_encrypted  BLOB,
    created_at_ms   INTEGER NOT NULL
);

-- Message skeletons (metadata only, body stored separately)
CREATE TABLE IF NOT EXISTS message_skeleton (
    message_id      TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    sender_id       TEXT NOT NULL,
    created_at_ms   INTEGER NOT NULL,
    received_at_ms  INTEGER NOT NULL,
    kind            TEXT NOT NULL,
    body_state      TEXT NOT NULL,
    media_state     TEXT,
    archive_state   TEXT NOT NULL DEFAULT 'not_archived',
    backup_state    TEXT NOT NULL DEFAULT 'not_backed_up',
    reply_to        TEXT,
    edited_at_ms    INTEGER,
    deleted_at_ms   INTEGER
);
CREATE INDEX IF NOT EXISTS idx_skeleton_conv_time
    ON message_skeleton (conversation_id, created_at_ms DESC);

-- Message bodies (plaintext, stored in SQLCipher)
CREATE TABLE IF NOT EXISTS message_body (
    message_id      TEXT PRIMARY KEY REFERENCES message_skeleton(message_id),
    text_content    TEXT,
    detected_language TEXT,
    rich_meta       BLOB
);

-- Media assets
CREATE TABLE IF NOT EXISTS media_asset (
    asset_id        TEXT PRIMARY KEY,
    message_id      TEXT NOT NULL REFERENCES message_skeleton(message_id),
    mime_type       TEXT NOT NULL,
    bytes_total     INTEGER NOT NULL,
    bytes_local     INTEGER NOT NULL DEFAULT 0,
    media_state     TEXT NOT NULL,
    wrapped_k_asset BLOB NOT NULL,
    chunk_count     INTEGER NOT NULL,
    merkle_root     BLOB NOT NULL,
    node_id         TEXT NOT NULL,
    version_id      TEXT NOT NULL,
    storage_sink    TEXT NOT NULL DEFAULT 'kchat_backend',
    created_at_ms   INTEGER NOT NULL DEFAULT 0
);

-- FTS5 full-text search index
CREATE VIRTUAL TABLE IF NOT EXISTS search_fts USING fts5(
    message_id      UNINDEXED,
    conversation_id UNINDEXED,
    sender_id       UNINDEXED,
    created_at_ms   UNINDEXED,
    text_content,
    tokenize = 'unicode61 remove_diacritics 2'
);

-- Fuzzy search index (trigrams/bigrams)
CREATE TABLE IF NOT EXISTS search_fuzzy (
    token       TEXT NOT NULL,
    script      TEXT NOT NULL,
    message_id  TEXT NOT NULL,
    PRIMARY KEY (token, script, message_id)
);

-- Vector search index (embeddings)
CREATE TABLE IF NOT EXISTS search_vector (
    message_id    TEXT NOT NULL,
    embedding     BLOB NOT NULL,
    model_version TEXT NOT NULL,
    PRIMARY KEY (message_id, model_version)
);

-- Media search index (OCR, captions, transcripts)
CREATE TABLE IF NOT EXISTS media_search_index (
    asset_id      TEXT NOT NULL,
    kind          TEXT NOT NULL,
    text          TEXT NOT NULL,
    language      TEXT,
    confidence    REAL,
    PRIMARY KEY (asset_id, kind, text)
);

-- Backup event journal
CREATE TABLE IF NOT EXISTS backup_event_journal (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type  TEXT NOT NULL,
    payload     BLOB NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Archive segment map
CREATE TABLE IF NOT EXISTS archive_segment_map (
    segment_id      TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    time_bucket     TEXT NOT NULL,
    epoch_id        INTEGER NOT NULL,
    storage_key     TEXT NOT NULL,
    size            INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_archive_conv_bucket
    ON archive_segment_map (conversation_id, time_bucket);

-- Restore state
CREATE TABLE IF NOT EXISTS restore_state (
    key     TEXT PRIMARY KEY,
    value   TEXT NOT NULL
);

-- Knowledge cards
CREATE TABLE IF NOT EXISTS knowledge_card (
    card_id         TEXT PRIMARY KEY,
    title           TEXT NOT NULL,
    card_type       TEXT NOT NULL,
    source_messages TEXT NOT NULL,
    confidence      REAL NOT NULL DEFAULT 0.0,
    pinned          INTEGER NOT NULL DEFAULT 0,
    created_at_ms   INTEGER NOT NULL
);

-- Knowledge entities
CREATE TABLE IF NOT EXISTS knowledge_entity (
    entity_id   TEXT PRIMARY KEY,
    card_id     TEXT NOT NULL REFERENCES knowledge_card(card_id),
    entity_type TEXT NOT NULL,
    value       TEXT NOT NULL,
    language    TEXT
);

-- Threat index
CREATE TABLE IF NOT EXISTS threat_index (
    threat_id   TEXT PRIMARY KEY,
    pattern     TEXT NOT NULL,
    severity    TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);

-- Outbox (pending outgoing messages)
CREATE TABLE IF NOT EXISTS outbox (
    client_message_id  TEXT PRIMARY KEY,
    conversation_id    TEXT NOT NULL,
    text_content       TEXT,
    media_asset_id     TEXT,
    created_at_ms      INTEGER NOT NULL,
    sent               INTEGER NOT NULL DEFAULT 0,
    sent_at_ms         INTEGER
);
"#;

/// Row struct for `message_skeleton` table.
#[derive(Debug, Clone)]
pub struct TimelineRow {
    pub message_id: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub created_at_ms: i64,
    pub kind: String,
    pub body_state: String,
    pub media_state: Option<String>,
    pub reply_to: Option<String>,
}

/// Row struct for `message_body` table.
#[derive(Debug, Clone)]
pub struct MessageBody {
    pub message_id: String,
    pub text_content: Option<String>,
    pub detected_language: Option<String>,
}

/// Row struct for `conversation` table.
#[derive(Debug, Clone)]
pub struct Conversation {
    pub id: String,
    pub conversation_type: String,
    pub scope: String,
    pub tenant_id: Option<String>,
    pub community_id: Option<String>,
    pub domain_id: Option<String>,
    pub name_encrypted: Option<Vec<u8>>,
    pub created_at_ms: i64,
}

/// Row struct for `media_asset` table.
#[derive(Debug, Clone)]
pub struct MediaAsset {
    pub asset_id: String,
    pub message_id: String,
    pub mime_type: String,
    pub bytes_total: i64,
    pub bytes_local: i64,
    pub media_state: String,
    pub chunk_count: i64,
    pub merkle_root: Vec<u8>,
    pub node_id: String,
    pub version_id: String,
    pub storage_sink: String,
    pub created_at_ms: i64,
}

/// Message kind enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Text,
    Media,
    System,
}

impl MessageKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageKind::Text => "text",
            MessageKind::Media => "media",
            MessageKind::System => "system",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "media" => MessageKind::Media,
            "system" => MessageKind::System,
            _ => MessageKind::Text,
        }
    }
}

/// Message skeleton row.
#[derive(Debug, Clone)]
pub struct MessageSkeleton {
    pub message_id: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub created_at_ms: i64,
    pub received_at_ms: i64,
    pub kind: MessageKind,
    pub body_state: String,
    pub media_state: Option<String>,
    pub archive_state: String,
    pub backup_state: String,
    pub reply_to: Option<String>,
    pub edited_at_ms: Option<i64>,
    pub deleted_at_ms: Option<i64>,
}

/// Backup event journal entry.
#[derive(Debug, Clone)]
pub struct BackupEventJournalEntry {
    pub seq: i64,
    pub event_type: String,
    pub payload: Vec<u8>,
}
