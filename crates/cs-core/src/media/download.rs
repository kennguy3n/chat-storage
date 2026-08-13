//! Media download via kdrive DriveFacade.

/// Download media via the kdrive DriveFacade.
pub fn download(
    _drive_facade: &kchat_client_runtime::facade::DriveFacade,
    _version_id: &kchat_drive_types::VersionId,
) -> Result<Vec<u8>, crate::Error> {
    Err(crate::Error::NotImplemented("media::download"))
}
