//! Error codes of `pawsman-net`.

use pawsman_error::ErrorCode;

/// The method, URL or a header is not valid.
pub const INVALID_REQUEST: ErrorCode = ErrorCode::new("net::invalid_request");
/// The host name does not resolve.
pub const DNS: ErrorCode = ErrorCode::new("net::dns");
/// The TCP connection failed.
pub const CONNECT: ErrorCode = ErrorCode::new("net::connect");
/// The TLS handshake or certificate check failed.
pub const TLS: ErrorCode = ErrorCode::new("net::tls");
/// The connection or the whole request took too long.
pub const TIMEOUT: ErrorCode = ErrorCode::new("net::timeout");
/// The server broke HTTP mid-request.
pub const PROTOCOL: ErrorCode = ErrorCode::new("net::protocol");
/// A large body could not be written to its temp file.
pub const SPILL: ErrorCode = ErrorCode::new("net::spill");

/// Every code, for the registry test.
pub const ALL: &[ErrorCode] = &[INVALID_REQUEST, DNS, CONNECT, TLS, TIMEOUT, PROTOCOL, SPILL];

#[cfg(test)]
mod tests {
    #[test]
    fn every_code_has_a_page() {
        pawsman_test_support::errors::assert_pages("net", super::ALL);
    }
}
