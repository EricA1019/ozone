//! Shared HTTP client construction for the ozone product family.
//!
//! Provides reusable client builders so callers don't duplicate
//! `reqwest::Client::builder()` boilerplate across the codebase.

use std::time::Duration;

/// Create an HTTP client with a specific overall timeout in seconds.
///
/// Returns an error if the underlying TLS backend fails to initialise,
/// which is extremely rare in practice and typically indicates a
/// system-level OpenSSL / native-tls misconfiguration.
pub fn client_with_timeout(secs: u64) -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(secs))
        .build()
}

/// Create an HTTP client with a default 30-second timeout.
pub fn default_client() -> reqwest::Result<reqwest::Client> {
    client_with_timeout(30)
}

/// Create an HTTP client with separate connect and overall timeouts.
///
/// - `connect_secs`: maximum time to wait for a TCP/TLS handshake.
/// - `timeout_secs`: maximum time for the entire request (including body).
pub fn client_with_timeouts(
    connect_secs: u64,
    timeout_secs: u64,
) -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(connect_secs))
        .timeout(Duration::from_secs(timeout_secs))
        .build()
}
