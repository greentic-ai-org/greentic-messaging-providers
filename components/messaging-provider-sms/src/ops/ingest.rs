use provider_common::http_compat::http_out_error;

/// Twilio inbound webhook parsing + signature validation land in a later
/// task of this epic; Task 1 only wires the op through as a stub.
pub(crate) fn ingest_http(_input_json: &[u8]) -> Vec<u8> {
    http_out_error(
        501,
        "messaging-provider-sms ingest_http not yet implemented",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_types::messaging::universal_dto::HttpOutV1;

    #[test]
    fn ingest_http_stub_returns_not_implemented() {
        let result = ingest_http(b"{}");
        let out: HttpOutV1 = serde_json::from_slice(&result).expect("valid HttpOutV1");
        assert_eq!(out.status, 501);
        assert!(out.events.is_empty());
    }
}
