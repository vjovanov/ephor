//! Telling "I could not reach the destination" apart from every other way a
//! provider fails.
//!
//! The distinction is the difference between two very different instructions
//! to the reader. A misconfigured provider, a crashed extension or a rejected
//! credential is something to go and fix; a host that cannot be reached
//! usually means a VPN is down or the network is out, and the feed will heal
//! by itself once it is back. Reporting both as "provider warning" leaves the
//! reader unable to tell a broken setup from a closed laptop lid, and the
//! items already on screen — kept from the last good fetch — look current
//! either way.
//!
//! Providers report failures as prose, because they wrap tools that do
//! (`gh`, `curl`, a vendor CLI, a JVM stack trace). So the classification is
//! done on the message, matching the phrases those tools actually print.

/// Phrases that mean the destination was never reached: DNS did not resolve,
/// the connection was refused or reset, or the route does not exist. Matched
/// case-insensitively against the whole failure message.
const UNREACHABLE: &[&str] = &[
    // Name resolution.
    "could not resolve host",
    "name or service not known",
    "temporary failure in name resolution",
    "nodename nor servname provided",
    "unknownhostexception",
    "no such host",
    // Refused, reset, or no route.
    "connection refused",
    "connection reset",
    "connection closed",
    "no route to host",
    "network is unreachable",
    "network unreachable",
    "host is unreachable",
    "connectexception",
    "econnrefused",
    "enetunreach",
    "ehostunreach",
    // Reached nothing before giving up. Bare "timed out" is deliberately not
    // here: a provider that is slow but working times out too, and calling
    // that unreachable would send the reader to check a network that is fine.
    "connection timed out",
    "connect timed out",
    "connection timeout",
    "sockettimeoutexception",
    "i/o timeout",
    "tls handshake timeout",
    "operation timed out",
    // Proxies and gateways in front of a destination that is not answering.
    "502 bad gateway",
    "503 service unavailable",
    "504 gateway",
    "bad gateway",
    "service unavailable",
];

/// Whether a provider's failure message says the destination was unreachable.
pub fn is_unreachable(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    UNREACHABLE.iter().any(|phrase| message.contains(phrase))
}

#[cfg(test)]
mod tests {
    use super::is_unreachable;

    #[test]
    fn recognises_what_the_tools_actually_print() {
        for message in [
            "gdev-cli bitbucket list-prs failed: java.net.UnknownHostException: ol-bitbucket.example.com",
            "dial tcp 140.82.121.6:443: connect: connection refused",
            "fatal: unable to access 'https://example.com/': Could not resolve host: example.com",
            "Get \"https://api.example.com\": net/http: TLS handshake timeout",
            "curl: (7) Failed to connect to host: No route to host",
            "upstream returned 503 Service Unavailable",
        ] {
            assert!(is_unreachable(message), "should be unreachable: {message}");
        }
    }

    /// The failures a reader has to act on themselves must not be filed as
    /// "the network will fix itself".
    #[test]
    fn does_not_claim_unreachable_for_failures_the_reader_must_fix() {
        for message in [
            "`ephor-forge-gdev` is not on PATH",
            "gh: authentication required, run `gh auth login`",
            "ephor-forge-gdev pull-requests: timed out after 180s",
            "output does not match the forge interface: missing field `id`",
            "HTTP 404: Not Found",
            "HTTP 403: rate limit exceeded",
        ] {
            assert!(!is_unreachable(message), "should not be unreachable: {message}");
        }
    }
}
