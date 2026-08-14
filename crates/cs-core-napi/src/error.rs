use cs_core::Error;

pub fn invalid_input(msg: String) -> napi::Error {
    napi::Error::new(napi::Status::InvalidArg, msg)
}

pub fn to_napi_error(e: Error) -> napi::Error {
    napi::Error::new(napi::Status::GenericFailure, e.to_string())
}
