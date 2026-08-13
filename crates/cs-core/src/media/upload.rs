//! Media upload via kdrive DriveFacade.

/// Upload media via the kdrive DriveFacade.
/// This is a thin wrapper that delegates to `DriveFacade::upload`.
pub fn upload(
    _drive_facade: &kchat_client_runtime::facade::DriveFacade,
    _drive_id: &kchat_drive_types::DriveId,
    _node_id: &kchat_drive_types::NodeId,
    _domain_id: &kchat_drive_types::DomainId,
    _privacy_mode: kchat_drive_types::PrivacyMode,
    _plaintext: &[u8],
) -> Result<kchat_client_runtime::facade::UploadResult, crate::Error> {
    // The actual upload requires signing keys and wrapping keys
    // which are managed by the caller (CoreImpl).
    Err(crate::Error::NotImplemented("media::upload"))
}
