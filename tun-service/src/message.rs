//! Text shown to the user crosses to the UI as a dictionary key (see `src/i18n`), never as prose,
//! so the frontend can show it in the language in use. A message without values is just its key,
//! e.g. `"native.tunCoreStopped"`; one with values is built by `msg`. Verbatim text such as an
//! OS error or a path only ever travels as a value.
use serde::Serialize;
use serde_json::{Map, Value};
use std::fmt::Display;

// A struct keeps `key` first: the frontend recognises a message by its `{"key":"` prefix.
#[derive(Serialize)]
struct Message<'a> {
    key: &'a str,
    values: Map<String, Value>,
}

/// The wire form of a message with values: `{"key":"…","values":{…}}`.
/// Values are strings or numbers. A string may itself be a key or another encoded message
/// (an inner error), which the frontend renders in turn.
pub fn msg(key: &str, values: &[(&str, Value)]) -> String {
    if values.is_empty() {
        return key.to_owned();
    }
    let values = values
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect();
    serde_json::to_string(&Message { key, values }).unwrap_or_else(|_| key.to_owned())
}

/// A text value: anything printable, such as an error, a status or a path.
pub fn text(value: impl Display) -> Value {
    Value::String(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builds_the_wire_form_the_frontend_decodes() {
        assert_eq!(msg("native.appQuitting", &[]), "native.appQuitting");
        assert_eq!(
            msg(
                "native.proxyStarted",
                &[("address", text("127.0.0.1")), ("port", 2080.into())]
            ),
            r#"{"key":"native.proxyStarted","values":{"address":"127.0.0.1","port":2080}}"#
        );
        // Verbatim text and a nested message both survive as JSON strings.
        let inner = msg(
            "native.probeHttpStatus",
            &[("status", text("502 \"Bad\"\nGateway"))],
        );
        let outer = msg("native.egressCheckFailed", &[("error", text(&inner))]);
        assert!(outer.starts_with(r#"{"key":"native.egressCheckFailed","values":{"error":"#));
        let parsed: Value = serde_json::from_str(&outer).unwrap();
        assert_eq!(parsed["values"]["error"], inner);
    }
}
