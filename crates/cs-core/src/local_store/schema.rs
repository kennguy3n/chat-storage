//! SQLCipher schema for the local store.
//!
//! The SQL in [`SCHEMA_SQL`] mirrors the design docs. The Rust structs
//! in this module are 1:1 with the schema columns, using the typed
//! enums from [`super::state_machines`] for state columns.

use serde::{Deserialize, Serialize};

use super::state_machines::{ArchiveState, BackupState, BodyState, MediaState, RestoreState};

// ---------------------------------------------------------------------------
// CREATE TABLE statements
// ---------------------------------------------------------------------------

/// Concatenated `CREATE TABLE` / `CREATE VIRTUAL TABLE` statements for
/// every table in the local store. Designed for
/// `connection.execute_batch(SCHEMA_SQL)`.
pub const SCHEMA_SQL: &str = r#"
-- Conversations
CREATE TABLE IF NOT EXISTS conversation (
    conversation_id   TEXT PRIMARY KEY,
    title_cipher      BLOB,                 -- encrypted with K_local_db
    pinned            INTEGER NOT NULL DEFAULT 0,
    muted             INTEGER NOT NULL DEFAULT 0,
    last_message_id   TEXT,
    last_activity_ms  INTEGER NOT NULL,
    conversation_type TEXT NOT NULL DEFAULT 'dm',
    scope             TEXT NOT NULL DEFAULT 'b2c',
    tenant_id         TEXT NOT NULL DEFAULT '',
    community_id      TEXT NOT NULL DEFAULT '',
    domain_id         TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_conv_community
    ON conversation(community_id);
CREATE INDEX IF NOT EXISTS idx_conv_domain
    ON conversation(domain_id);
CREATE INDEX IF NOT EXISTS idx_conv_tenant
    ON conversation(tenant_id);
CREATE INDEX IF NOT EXISTS idx_conv_scope
    ON conversation(scope);

-- Skeletons render the timeline before any body / media is loaded
CREATE TABLE IF NOT EXISTS message_skeleton (
    message_id        TEXT PRIMARY KEY,
    conversation_id   TEXT NOT NULL REFERENCES conversation(conversation_id),
    sender_id         TEXT NOT NULL,
    created_at_ms     INTEGER NOT NULL,
    received_at_ms    INTEGER NOT NULL,
    kind              TEXT NOT NULL,
    body_state        TEXT NOT NULL,
    media_state       TEXT,
    archive_state     TEXT NOT NULL DEFAULT 'not_archived',
    backup_state      TEXT NOT NULL DEFAULT 'not_backed_up',
    reply_to          TEXT,
    edited_at_ms      INTEGER,
    deleted_at_ms     INTEGER
);
CREATE INDEX IF NOT EXISTS idx_skeleton_conv_time
    ON message_skeleton (conversation_id, created_at_ms DESC);

-- M17: Additional indexes for backup, archive, and sender queries
CREATE INDEX IF NOT EXISTS idx_skeleton_backup_state
    ON message_skeleton(backup_state) WHERE backup_state = 'not_backed_up';
CREATE INDEX IF NOT EXISTS idx_skeleton_archive_state
    ON message_skeleton(archive_state, created_at_ms);
CREATE INDEX IF NOT EXISTS idx_skeleton_sender
    ON message_skeleton(conversation_id, sender_id, created_at_ms);

CREATE TABLE IF NOT EXISTS message_body (
    message_id        TEXT PRIMARY KEY REFERENCES message_skeleton(message_id),
    text_content      TEXT,                 -- UTF-8, may mix scripts
    detected_language TEXT,                 -- BCP-47, optional
    rich_meta         BLOB                  -- mentions, link previews (CBOR)
);

CREATE TABLE IF NOT EXISTS media_asset (
    asset_id          TEXT PRIMARY KEY,
    message_id        TEXT NOT NULL REFERENCES message_skeleton(message_id),
    mime_type         TEXT NOT NULL,
    bytes_total       INTEGER NOT NULL,
    bytes_local       INTEGER NOT NULL,
    media_state       TEXT NOT NULL,
    wrapped_k_asset   BLOB NOT NULL,
    chunk_count       INTEGER NOT NULL,
    merkle_root       BLOB NOT NULL,
    blob_id           TEXT NOT NULL,
    storage_sink      TEXT NOT NULL DEFAULT 'kchat_backend'
);

-- Multilingual full-text search
CREATE VIRTUAL TABLE IF NOT EXISTS search_fts USING fts5(
    message_id        UNINDEXED,
    conversation_id   UNINDEXED,
    sender_id         UNINDEXED,
    created_at_ms     UNINDEXED,
    text_content,
    tokenize = 'unicode61 remove_diacritics 2',
    detail = 'full'
);

CREATE TABLE IF NOT EXISTS search_fuzzy (
    token       TEXT NOT NULL,
    script      TEXT NOT NULL,             -- ISO-15924
    message_id  TEXT NOT NULL,
    PRIMARY KEY (token, script, message_id)
);

CREATE TABLE IF NOT EXISTS search_vector (
    message_id    TEXT NOT NULL,
    embedding     BLOB NOT NULL,            -- INT8-quantized
    model_version TEXT NOT NULL,
    PRIMARY KEY (message_id, model_version)
);

CREATE TABLE IF NOT EXISTS media_search_index (
    asset_id      TEXT NOT NULL REFERENCES media_asset(asset_id),
    kind          TEXT NOT NULL,            -- 'ocr' | 'caption' | 'transcript' | 'tag'
    text          TEXT NOT NULL,
    language      TEXT,                     -- BCP-47 if detected
    confidence    REAL,
    PRIMARY KEY (asset_id, kind, text)
);

-- Backup pipeline
CREATE TABLE IF NOT EXISTS backup_event_journal (
    event_seq       INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type      TEXT NOT NULL,
    conversation_id TEXT,
    message_id      TEXT,
    payload         BLOB NOT NULL,            -- CBOR
    created_at_ms   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_backup_event_journal_created
    ON backup_event_journal(created_at_ms);

-- m6: Persisted backup coordinator state (single-row table)
CREATE TABLE IF NOT EXISTS backup_state (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    current_generation  INTEGER NOT NULL,
    prev_manifest_hash  BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS backup_event_cursor (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    cursor_seq  INTEGER NOT NULL DEFAULT 0
);

-- Archive pipeline
CREATE TABLE IF NOT EXISTS archive_segment_map (
    segment_id           TEXT PRIMARY KEY,
    conversation_id      TEXT NOT NULL,
    time_bucket          TEXT NOT NULL,
    segment_type         TEXT NOT NULL,
    blob_id              TEXT NOT NULL,
    storage_backend      TEXT NOT NULL DEFAULT 'kchat_backend',
    merkle_root          BLOB NOT NULL,
    state                TEXT NOT NULL,
    tenant_id            TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_asm_tenant_bucket
    ON archive_segment_map(tenant_id, time_bucket);
CREATE INDEX IF NOT EXISTS idx_archive_conv_bucket
    ON archive_segment_map (conversation_id, time_bucket);

CREATE TABLE IF NOT EXISTS archive_event_journal (
    event_seq       INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type      TEXT NOT NULL,
    conversation_id TEXT NOT NULL,
    message_id      TEXT,
    payload         BLOB NOT NULL,            -- CBOR
    created_at_ms   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS archive_event_cursor (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    cursor_seq  INTEGER NOT NULL DEFAULT 0
);

-- Restore state machine
CREATE TABLE IF NOT EXISTS restore_state (
    id     INTEGER PRIMARY KEY CHECK (id = 1),
    state  TEXT NOT NULL,
    notes  TEXT
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

/// All tables defined in [`SCHEMA_SQL`], in declaration order.
pub const TABLES: &[&str] = &[
    "conversation",
    "message_skeleton",
    "message_body",
    "media_asset",
    "search_fts",
    "search_fuzzy",
    "search_vector",
    "media_search_index",
    "backup_event_journal",
    "backup_event_cursor",
    "backup_state",
    "archive_segment_map",
    "archive_event_journal",
    "archive_event_cursor",
    "restore_state",
    "knowledge_card",
    "knowledge_entity",
    "threat_index",
    "outbox",
];

// ---------------------------------------------------------------------------
// Forward-only migrations
// ---------------------------------------------------------------------------

/// Migration v2 — backup manifest chain and segment ledger tables.
pub const MIGRATION_V2_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS backup_manifest_chain (
    id              INTEGER PRIMARY KEY CHECK (id = 1),
    generation      INTEGER NOT NULL,
    manifest_cbor   BLOB NOT NULL,
    updated_at_ms   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS backup_segment_ledger (
    segment_id           TEXT PRIMARY KEY,
    segment_type         TEXT NOT NULL,
    nonce                BLOB NOT NULL,
    ciphertext           BLOB NOT NULL,
    merkle_root          BLOB NOT NULL,
    event_count          INTEGER NOT NULL,
    tier                 TEXT NOT NULL,
    min_event_ms         INTEGER NOT NULL,
    max_event_ms         INTEGER NOT NULL,
    wrapped_k_segment    BLOB NOT NULL,
    created_at_ms        INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_backup_segment_ledger_tier_min
    ON backup_segment_ledger(tier, min_event_ms);
"#;

/// Migration v3 — add `sink_metadata` column to `media_asset`.
pub const MIGRATION_V3_SQL: &str = r#"
ALTER TABLE media_asset ADD COLUMN sink_metadata BLOB;
"#;

/// Migration v4 — search shard map ledger.
pub const MIGRATION_V4_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS search_shard_map (
    conversation_hash   TEXT NOT NULL,
    time_bucket         TEXT NOT NULL,
    shard_type          TEXT NOT NULL,
    shard_id            TEXT NOT NULL,
    doc_count           INTEGER NOT NULL,
    ciphertext_len      INTEGER NOT NULL,
    ciphertext_sha256   BLOB NOT NULL,
    uploaded_at_ms      INTEGER NOT NULL,
    PRIMARY KEY (conversation_hash, time_bucket, shard_type)
);
"#;

/// Migration v5 — add `content_hash` to `search_shard_map`.
pub const MIGRATION_V5_SQL: &str = r#"
ALTER TABLE search_shard_map ADD COLUMN content_hash BLOB;
"#;

/// Ordered table of forward migrations.
pub const MIGRATIONS: &[(i32, &str)] = &[
    (1, SCHEMA_SQL),
    (2, MIGRATION_V2_SQL),
    (3, MIGRATION_V3_SQL),
    (4, MIGRATION_V4_SQL),
    (5, MIGRATION_V5_SQL),
];

/// The highest `user_version` produced by [`MIGRATIONS`].
pub const LATEST_USER_VERSION: i32 = 5;

// ---------------------------------------------------------------------------
// Row types
// ---------------------------------------------------------------------------

/// `kind` discriminator for `message_skeleton.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    /// Plain UTF-8 text body.
    Text,
    /// Media body (image / video / audio / document).
    Media,
    /// System / control message (group join, subject change, etc.).
    System,
}

impl MessageKind {
    /// Canonical snake_case representation used in the SQL column.
    pub fn as_str(self) -> &'static str {
        match self {
            MessageKind::Text => "text",
            MessageKind::Media => "media",
            MessageKind::System => "system",
        }
    }

    /// Parse a string into a `MessageKind`. Falls back to `Text` for
    /// unknown values (matching legacy behavior).
    pub fn parse(s: &str) -> Self {
        match s {
            "media" => MessageKind::Media,
            "system" => MessageKind::System,
            _ => MessageKind::Text,
        }
    }
}

/// Typed value of `archive_segment_map.storage_backend`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StorageBackend {
    #[default]
    #[serde(rename = "kchat_backend")]
    KChatBackend,
    #[serde(rename = "zk_object_fabric")]
    ZkObjectFabric,
}

impl StorageBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            StorageBackend::KChatBackend => "kchat_backend",
            StorageBackend::ZkObjectFabric => "zk_object_fabric",
        }
    }
}

impl std::fmt::Display for StorageBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for StorageBackend {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "kchat_backend" => Ok(StorageBackend::KChatBackend),
            "zk_object_fabric" => Ok(StorageBackend::ZkObjectFabric),
            other => Err(format!("invalid storage_backend value: {other:?}")),
        }
    }
}

impl From<crate::config::ArchiveBackend> for StorageBackend {
    fn from(backend: crate::config::ArchiveBackend) -> Self {
        match backend {
            crate::config::ArchiveBackend::Kdrive => StorageBackend::KChatBackend,
            crate::config::ArchiveBackend::Zkof => StorageBackend::ZkObjectFabric,
        }
    }
}

/// `conversation` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversation {
    pub conversation_id: String,
    pub title_cipher: Option<Vec<u8>>,
    pub pinned: bool,
    pub muted: bool,
    pub last_message_id: Option<String>,
    pub last_activity_ms: i64,
    #[serde(default = "default_conversation_type")]
    pub conversation_type: String,
    #[serde(default = "default_scope")]
    pub scope: String,
    #[serde(default)]
    pub tenant_id: String,
    #[serde(default)]
    pub community_id: String,
    #[serde(default)]
    pub domain_id: String,
}

fn default_conversation_type() -> String {
    "dm".into()
}

fn default_scope() -> String {
    "b2c".into()
}

impl Conversation {
    /// Build a legacy-style conversation row with default hierarchy.
    pub fn legacy(
        conversation_id: impl Into<String>,
        title_cipher: Option<Vec<u8>>,
        pinned: bool,
        muted: bool,
        last_message_id: Option<String>,
        last_activity_ms: i64,
    ) -> Self {
        Self {
            conversation_id: conversation_id.into(),
            title_cipher,
            pinned,
            muted,
            last_message_id,
            last_activity_ms,
            conversation_type: default_conversation_type(),
            scope: default_scope(),
            tenant_id: String::new(),
            community_id: String::new(),
            domain_id: String::new(),
        }
    }
}

impl Default for Conversation {
    fn default() -> Self {
        Self::legacy(String::new(), None, false, false, None, 0)
    }
}

/// `message_skeleton` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageSkeleton {
    pub message_id: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub created_at_ms: i64,
    pub received_at_ms: i64,
    pub kind: MessageKind,
    pub body_state: BodyState,
    pub media_state: Option<MediaState>,
    pub archive_state: ArchiveState,
    pub backup_state: BackupState,
    pub reply_to: Option<String>,
    pub edited_at_ms: Option<i64>,
    pub deleted_at_ms: Option<i64>,
}

/// `message_body` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageBody {
    pub message_id: String,
    pub text_content: Option<String>,
    pub detected_language: Option<String>,
    pub rich_meta: Option<Vec<u8>>,
}

/// `media_asset` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaAsset {
    pub asset_id: String,
    pub message_id: String,
    pub mime_type: String,
    pub bytes_total: i64,
    pub bytes_local: i64,
    pub media_state: MediaState,
    pub wrapped_k_asset: Vec<u8>,
    pub chunk_count: i32,
    pub merkle_root: Vec<u8>,
    pub blob_id: String,
    pub storage_sink: String,
}

/// `backup_event_journal` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupEventJournalEntry {
    pub event_seq: i64,
    pub event_type: String,
    pub conversation_id: Option<String>,
    pub message_id: Option<String>,
    pub payload: Vec<u8>,
    pub created_at_ms: i64,
}

/// `archive_segment_map` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveSegmentMapEntry {
    pub segment_id: String,
    pub conversation_id: String,
    pub time_bucket: String,
    pub segment_type: String,
    pub blob_id: String,
    pub storage_backend: String,
    pub merkle_root: Vec<u8>,
    pub state: ArchiveState,
}

/// `restore_state` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreStateEntry {
    pub id: i32,
    pub state: RestoreState,
    pub notes: Option<String>,
}

/// Timeline view row returned by `get_timeline`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineRow {
    pub message_id: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub created_at_ms: i64,
    pub kind: MessageKind,
    pub body_state: BodyState,
    pub text_content: Option<String>,
    pub reply_to: Option<String>,
    pub edited_at_ms: Option<i64>,
    pub deleted_at_ms: Option<i64>,
}
