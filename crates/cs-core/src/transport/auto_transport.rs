//! Auto transport selection — picks the right transport based on platform.

pub fn auto_select_transport(
    base_url: &str,
    auth_token: &str,
    tenant_id: &str,
    user_id: &str,
) -> crate::transport::kdrive_bridge::KdriveTransport {
    crate::transport::kdrive_bridge::KdriveTransport::new(
        base_url.to_string(),
        auth_token.to_string(),
        tenant_id.to_string(),
        user_id.to_string(),
    )
}
