//! State machines for message body, media, archive, backup, and restore.
//!
//! Ported from `chat-storage-search` §4. Each enum has `try_transition`,
//! `Display`/`FromStr`, and serde support.

use serde::{Deserialize, Serialize};
use std::str::FromStr;

use super::StorageError;

// ---------------------------------------------------------------------------
// BodyState
// ---------------------------------------------------------------------------

/// State of a message body in the local store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BodyState {
    LocalPlainAvailable,
    LocalEncryptedAvailable,
    RemoteArchiveOnly,
    DeliveryStoreOnly,
    DeletedForMe,
    DeletedForEveryone,
    Unavailable,
}

impl BodyState {
    pub fn as_str(&self) -> &'static str {
        match self {
            BodyState::LocalPlainAvailable => "local_plain_available",
            BodyState::LocalEncryptedAvailable => "local_encrypted_available",
            BodyState::RemoteArchiveOnly => "remote_archive_only",
            BodyState::DeliveryStoreOnly => "delivery_store_only",
            BodyState::DeletedForMe => "deleted_for_me",
            BodyState::DeletedForEveryone => "deleted_for_everyone",
            BodyState::Unavailable => "unavailable",
        }
    }

    /// Attempt a state transition. Returns `Err` if the transition is invalid.
    pub fn try_transition(&self, target: BodyState) -> Result<BodyState, StorageError> {
        use BodyState::*;
        if self == &target {
            return Ok(target);
        }
        let allowed = matches!(
            (self, target),
            (DeliveryStoreOnly, LocalPlainAvailable)
                | (DeliveryStoreOnly, LocalEncryptedAvailable)
                | (DeliveryStoreOnly, RemoteArchiveOnly)
                | (DeliveryStoreOnly, DeletedForMe)
                | (DeliveryStoreOnly, DeletedForEveryone)
                | (DeliveryStoreOnly, Unavailable)
                | (LocalPlainAvailable, LocalEncryptedAvailable)
                | (LocalPlainAvailable, RemoteArchiveOnly)
                | (LocalPlainAvailable, DeletedForMe)
                | (LocalPlainAvailable, DeletedForEveryone)
                | (LocalPlainAvailable, Unavailable)
                | (LocalEncryptedAvailable, LocalPlainAvailable)
                | (LocalEncryptedAvailable, RemoteArchiveOnly)
                | (LocalEncryptedAvailable, DeletedForMe)
                | (LocalEncryptedAvailable, DeletedForEveryone)
                | (LocalEncryptedAvailable, Unavailable)
                | (RemoteArchiveOnly, LocalPlainAvailable)
                | (RemoteArchiveOnly, LocalEncryptedAvailable)
                | (RemoteArchiveOnly, Unavailable)
                | (DeletedForMe, DeletedForEveryone)
        );
        if allowed {
            Ok(target)
        } else {
            Err(StorageError::InvalidStateTransition(format!(
                "{} -> {}",
                self.as_str(),
                target.as_str()
            )))
        }
    }
}

impl FromStr for BodyState {
    type Err = StorageError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "local_plain_available" => Ok(BodyState::LocalPlainAvailable),
            "local_encrypted_available" => Ok(BodyState::LocalEncryptedAvailable),
            "remote_archive_only" => Ok(BodyState::RemoteArchiveOnly),
            "delivery_store_only" => Ok(BodyState::DeliveryStoreOnly),
            "deleted_for_me" => Ok(BodyState::DeletedForMe),
            "deleted_for_everyone" => Ok(BodyState::DeletedForEveryone),
            "unavailable" => Ok(BodyState::Unavailable),
            _ => Err(StorageError::Custom(format!("unknown body_state: {s}"))),
        }
    }
}

impl std::fmt::Display for BodyState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// MediaState
// ---------------------------------------------------------------------------

/// State of a media asset in the local store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaState {
    ThumbnailOnly,
    OriginalLocal,
    RemoteOriginal,
    DownloadInProgress,
    Evicted,
    Deleted,
}

impl MediaState {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaState::ThumbnailOnly => "thumbnail_only",
            MediaState::OriginalLocal => "original_local",
            MediaState::RemoteOriginal => "remote_original",
            MediaState::DownloadInProgress => "download_in_progress",
            MediaState::Evicted => "evicted",
            MediaState::Deleted => "deleted",
        }
    }

    pub fn try_transition(&self, target: MediaState) -> Result<MediaState, StorageError> {
        use MediaState::*;
        if self == &target {
            return Ok(target);
        }
        let allowed = matches!(
            (self, target),
            (ThumbnailOnly, OriginalLocal)
                | (ThumbnailOnly, RemoteOriginal)
                | (ThumbnailOnly, DownloadInProgress)
                | (ThumbnailOnly, Evicted)
                | (ThumbnailOnly, Deleted)
                | (OriginalLocal, ThumbnailOnly)
                | (OriginalLocal, RemoteOriginal)
                | (OriginalLocal, Evicted)
                | (OriginalLocal, Deleted)
                | (RemoteOriginal, DownloadInProgress)
                | (RemoteOriginal, OriginalLocal)
                | (RemoteOriginal, Evicted)
                | (RemoteOriginal, Deleted)
                | (DownloadInProgress, OriginalLocal)
                | (DownloadInProgress, RemoteOriginal)
                | (DownloadInProgress, Deleted)
                | (Evicted, DownloadInProgress)
                | (Evicted, Deleted)
        );
        if allowed {
            Ok(target)
        } else {
            Err(StorageError::InvalidStateTransition(format!(
                "{} -> {}",
                self.as_str(),
                target.as_str()
            )))
        }
    }
}

impl FromStr for MediaState {
    type Err = StorageError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "thumbnail_only" => Ok(MediaState::ThumbnailOnly),
            "original_local" => Ok(MediaState::OriginalLocal),
            "remote_original" => Ok(MediaState::RemoteOriginal),
            "download_in_progress" => Ok(MediaState::DownloadInProgress),
            "evicted" => Ok(MediaState::Evicted),
            "deleted" => Ok(MediaState::Deleted),
            _ => Err(StorageError::Custom(format!("unknown media_state: {s}"))),
        }
    }
}

impl std::fmt::Display for MediaState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// ArchiveState
// ---------------------------------------------------------------------------

/// Archive state of a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArchiveState {
    NotArchived,
    Archiving,
    Archived,
    ArchiveFailed,
}

impl ArchiveState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ArchiveState::NotArchived => "not_archived",
            ArchiveState::Archiving => "archiving",
            ArchiveState::Archived => "archived",
            ArchiveState::ArchiveFailed => "archive_failed",
        }
    }

    pub fn try_transition(&self, target: ArchiveState) -> Result<ArchiveState, StorageError> {
        use ArchiveState::*;
        if self == &target {
            return Ok(target);
        }
        let allowed = matches!(
            (self, target),
            (NotArchived, Archiving)
                | (Archiving, Archived)
                | (Archiving, ArchiveFailed)
                | (ArchiveFailed, Archiving)
                | (Archived, NotArchived)
        );
        if allowed {
            Ok(target)
        } else {
            Err(StorageError::InvalidStateTransition(format!(
                "{} -> {}",
                self.as_str(),
                target.as_str()
            )))
        }
    }
}

impl FromStr for ArchiveState {
    type Err = StorageError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "not_archived" => Ok(ArchiveState::NotArchived),
            "archiving" => Ok(ArchiveState::Archiving),
            "archived" => Ok(ArchiveState::Archived),
            "archive_failed" => Ok(ArchiveState::ArchiveFailed),
            _ => Err(StorageError::Custom(format!("unknown archive_state: {s}"))),
        }
    }
}

impl std::fmt::Display for ArchiveState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// BackupState
// ---------------------------------------------------------------------------

/// Backup state of a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BackupState {
    NotBackedUp,
    BackingUp,
    BackedUp,
    BackupFailed,
}

impl BackupState {
    pub fn as_str(&self) -> &'static str {
        match self {
            BackupState::NotBackedUp => "not_backed_up",
            BackupState::BackingUp => "backing_up",
            BackupState::BackedUp => "backed_up",
            BackupState::BackupFailed => "backup_failed",
        }
    }

    pub fn try_transition(&self, target: BackupState) -> Result<BackupState, StorageError> {
        use BackupState::*;
        if self == &target {
            return Ok(target);
        }
        let allowed = matches!(
            (self, target),
            (NotBackedUp, BackingUp)
                | (BackingUp, BackedUp)
                | (BackingUp, BackupFailed)
                | (BackupFailed, BackingUp)
                | (BackedUp, NotBackedUp)
        );
        if allowed {
            Ok(target)
        } else {
            Err(StorageError::InvalidStateTransition(format!(
                "{} -> {}",
                self.as_str(),
                target.as_str()
            )))
        }
    }
}

impl FromStr for BackupState {
    type Err = StorageError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "not_backed_up" => Ok(BackupState::NotBackedUp),
            "backing_up" => Ok(BackupState::BackingUp),
            "backed_up" => Ok(BackupState::BackedUp),
            "backup_failed" => Ok(BackupState::BackupFailed),
            _ => Err(StorageError::Custom(format!("unknown backup_state: {s}"))),
        }
    }
}

impl std::fmt::Display for BackupState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// RestoreState
// ---------------------------------------------------------------------------

/// State of a restore operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RestoreState {
    NotStarted,
    FetchingManifests,
    FetchingSkeletons,
    FetchingBodies,
    FetchingMedia,
    BuildingIndexes,
    Complete,
    Failed,
}

impl RestoreState {
    pub fn as_str(&self) -> &'static str {
        match self {
            RestoreState::NotStarted => "not_started",
            RestoreState::FetchingManifests => "fetching_manifests",
            RestoreState::FetchingSkeletons => "fetching_skeletons",
            RestoreState::FetchingBodies => "fetching_bodies",
            RestoreState::FetchingMedia => "fetching_media",
            RestoreState::BuildingIndexes => "building_indexes",
            RestoreState::Complete => "complete",
            RestoreState::Failed => "failed",
        }
    }
}

impl std::fmt::Display for RestoreState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_body_state_transitions() {
        let s = BodyState::DeliveryStoreOnly;
        assert!(s.try_transition(BodyState::LocalPlainAvailable).is_ok());
        assert!(s.try_transition(BodyState::DeletedForEveryone).is_ok());
        assert!(s.try_transition(BodyState::Unavailable).is_ok());
        // Invalid: can't go from DeletedForEveryone back to LocalPlainAvailable
        let d = BodyState::DeletedForEveryone;
        assert!(d.try_transition(BodyState::LocalPlainAvailable).is_err());
    }

    #[test]
    fn test_body_state_self_transition() {
        let s = BodyState::LocalPlainAvailable;
        assert!(s.try_transition(BodyState::LocalPlainAvailable).is_ok());
    }

    #[test]
    fn test_media_state_transitions() {
        let s = MediaState::ThumbnailOnly;
        assert!(s.try_transition(MediaState::OriginalLocal).is_ok());
        assert!(s.try_transition(MediaState::DownloadInProgress).is_ok());
        assert!(s.try_transition(MediaState::Deleted).is_ok());
    }

    #[test]
    fn test_archive_state_transitions() {
        let s = ArchiveState::NotArchived;
        assert!(s.try_transition(ArchiveState::Archiving).is_ok());
        assert!(s.try_transition(ArchiveState::Archived).is_err());
    }

    #[test]
    fn test_state_display_roundtrip() {
        let s = BodyState::LocalPlainAvailable;
        let str = s.to_string();
        let parsed: BodyState = str.parse().unwrap();
        assert_eq!(s, parsed);
    }
}
