//! Keeping other people's web pages out of this one's server.
//!
//! The program serves an HTTP API on a port of this machine with no password
//! on it, because everything it answers already belongs to whoever is sitting
//! here. That reasoning holds only as long as the thing asking is this
//! program's own interface, and a browser will happily let any page on the
//! internet ask instead. Two ways in, and both are closed here.
//!
//! **A name that resolves here.** A page at `evil.example` can have its name
//! answer `127.0.0.1` a minute after it was loaded; the browser then treats
//! this server's replies as that site's own and hands them over. What the
//! attack cannot do is fake an address - rebinding is a DNS trick, and
//! `127.0.0.1` is not a DNS name. So a request whose `Host` is a name other
//! than `localhost` is not from anyone who could have meant this server.
//!
//! **A form on another page.** A `POST` with no unusual headers is sent
//! without asking permission first, and the reply being unreadable does not
//! undo it: the update would have been applied, the model switched on, the
//! corpus fetched. Browsers say where such a request came from, so anything
//! that changes something has to come from here.

use axum::extract::Request;
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

pub async fn only_this_machine(request: Request, next: Next) -> Response {
    let headers = request.headers();
    // Something that did not say where it was going cannot have been sent by
    // a browser, which is the only thing either of these guards is about.
    let host = authority(headers, header::HOST)
        .or_else(|| request.uri().authority().map(|a| a.as_str().to_ascii_lowercase()));
    if let Some(host) = &host
        && !reachable_by_name(host)
    {
        return refuse("this server answers to its address, not to a name").into_response();
    }

    // A read cannot be replayed into a change, and the rule above is what
    // stops one being read by somebody else.
    if !matches!(*request.method(), Method::GET | Method::HEAD | Method::OPTIONS)
        && let Some(origin) = authority(headers, header::ORIGIN)
        && Some(&origin) != host.as_ref()
    {
        return refuse("this only takes requests from its own pages").into_response();
    }

    next.run(request).await
}

fn refuse(why: &str) -> (StatusCode, String) {
    (StatusCode::FORBIDDEN, why.to_string())
}

/// The `host:port` of a header, lower-cased. `Origin` carries a scheme too,
/// which is dropped: it is the name that decides this, not http or https.
fn authority(headers: &HeaderMap, name: header::HeaderName) -> Option<String> {
    let value = headers.get(&name)?.to_str().ok()?.trim();
    // `null` is what a sandboxed page sends, and it is nobody's address.
    if value.is_empty() || value == "null" {
        return None;
    }
    let value = value.split_once("://").map_or(value, |(_, rest)| rest);
    Some(value.to_ascii_lowercase())
}

/// Whether `authority` is one only this machine can be reached by.
///
/// An address is safe because it cannot be rebound; `localhost` is safe
/// because it is not resolved on the network. Every other name is somebody
/// else's, however it currently resolves.
fn reachable_by_name(authority: &str) -> bool {
    let host = match authority.rsplit_once(':') {
        // `[::1]:8420`, and a bare `[::1]`.
        _ if authority.starts_with('[') => {
            authority.split(']').next().map_or(authority, |h| &h[1..])
        }
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => host,
        _ => authority,
    };
    host == "localhost" || host.parse::<std::net::IpAddr>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_this_machine_however_it_is_written() {
        assert!(reachable_by_name("127.0.0.1:8420"));
        assert!(reachable_by_name("127.0.0.1"));
        assert!(reachable_by_name("localhost:8420"));
        assert!(reachable_by_name("localhost"));
        assert!(reachable_by_name("[::1]:8420"));
        assert!(reachable_by_name("[::1]"));
        // Somebody chose to publish it on their network; that is an address
        // too, and no name can be rebound onto it.
        assert!(reachable_by_name("192.168.1.5:8420"));
    }

    #[test]
    fn a_name_that_merely_resolves_here_is_not() {
        assert!(!reachable_by_name("evil.example"));
        assert!(!reachable_by_name("evil.example:8420"));
        assert!(!reachable_by_name("tsuburu.local"));
        // A name that looks like an address until it is read.
        assert!(!reachable_by_name("127.0.0.1.evil.example"));
    }

    #[test]
    fn an_origin_is_compared_without_its_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, "http://127.0.0.1:8420".parse().unwrap());
        assert_eq!(authority(&headers, header::ORIGIN).as_deref(), Some("127.0.0.1:8420"));
    }

    #[test]
    fn a_page_with_no_origin_of_its_own_is_not_treated_as_this_one() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, "null".parse().unwrap());
        assert_eq!(authority(&headers, header::ORIGIN), None);
    }
}
