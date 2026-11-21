use reqwest::Url;
fn local_http(url: &Url) -> bool {
    url.scheme() == "http" && matches!(url.host_str(), Some("localhost" | "127.0.0.1"))
}
pub fn validate(raw: &str) -> Result<Url, String> {
    if raw.len() > 8192 || raw.chars().any(char::is_control) {
        return Err("native.subscriptionUrlInvalid".into());
    }
    let url = Url::parse(raw).map_err(|_| "native.subscriptionUrlInvalid")?;
    if url.scheme() == "https" || local_http(&url) {
        Ok(url)
    } else {
        Err("native.subscriptionUrlNeedsHttps".into())
    }
}
pub fn redirect_allowed(initial: &Url, next: &Url) -> bool {
    // A remote subscription cannot use a redirect as a bridge into an insecure local service.
    next.scheme() == "https"
        || (local_http(initial) && local_http(next) && initial.origin() == next.origin())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_https_cannot_downgrade_into_loopback() {
        let source = validate("https://example.com/subscribe").unwrap();
        for url in [
            "http://127.0.0.1:8080/delete",
            "http://localhost:8080",
            "http://example.com",
            "file:///etc/passwd",
        ] {
            assert!(!redirect_allowed(&source, &Url::parse(url).unwrap()));
        }
        assert!(redirect_allowed(
            &source,
            &Url::parse("https://cdn.example.com/subscribe").unwrap()
        ));
    }
    #[test]
    fn explicit_local_subscriptions_only_redirect_within_the_same_http_origin() {
        let source = validate("http://127.0.0.1:8080/sub").unwrap();
        assert!(redirect_allowed(
            &source,
            &Url::parse("http://127.0.0.1:8080/updated").unwrap()
        ));
        assert!(!redirect_allowed(
            &source,
            &Url::parse("http://127.0.0.1:9090/admin").unwrap()
        ));
        assert!(validate("https://example.com/\n").is_err());
    }
}
