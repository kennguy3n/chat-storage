use cs_core::Error;

pub fn invalid_input(msg: String) -> napi::Error {
    napi::Error::new(napi::Status::InvalidArg, msg)
}

/// m8: Map specific `cs_core::Error` variants to appropriate `napi::Status` codes.
///
/// - `Error::Storage` / `Error::Crypto` → `napi::Status::GenericFailure`
/// - `Error::Search` / `Error::Message` / `Error::Transport` / `Error::Model`
///   / `Error::Tenant` → `napi::Status::GenericFailure`
/// - `Error::QuotaExceeded` → `napi::Status::GenericFailure`
/// - `Error::NotImplemented` → `napi::Status::GenericFailure`
///
/// "NotFound" conditions are surfaced as `Error::Storage` with a
/// "not found" message in the core library; these are mapped to
/// `napi::Status::ItemNotInCollection`.
pub fn to_napi_error(e: Error) -> napi::Error {
    let status = match &e {
        // Validation-style errors → InvalidArg
        Error::Search(cs_core::search::SearchError::QueryParse(_)) => napi::Status::InvalidArg,
        Error::Message(cs_core::message::MessageError::Validation(_)) => {
            napi::Status::InvalidArg
        }
        // NotFound → GenericFailure (napi 3.x has no ItemNotInCollection)
        Error::Storage(ref s) if s.to_string().contains("not found") => {
            napi::Status::GenericFailure
        }
        // Everything else → GenericFailure
        _ => napi::Status::GenericFailure,
    };
    napi::Error::new(status, e.to_string())
}
