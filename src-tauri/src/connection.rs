use crate::message::msg;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Phase {
    Idle,
    Preparing,
    Connecting,
    Connected,
    Reconnecting,
    Disconnecting,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnEvent {
    Start,
    CoreSpawned,
    CoreReady,
    CoreExited,
    Stop,
    Stopped,
    Fail,
    Retry,
}

#[derive(Deserialize)]
struct Vectors {
    transitions: Vec<Transition>,
}

#[derive(Deserialize)]
struct Transition {
    from: Phase,
    event: ConnEvent,
    to: Phase,
}

fn table() -> &'static Vec<Transition> {
    static TABLE: OnceLock<Vec<Transition>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let raw = include_str!("../../../contracts/connection-state.vectors.json");
        let parsed: Vectors =
            serde_json::from_str(raw).expect("connection state contract cannot be parsed");
        parsed.transitions
    })
}

pub fn reduce(from: Phase, event: ConnEvent) -> Result<Phase, String> {
    table()
        .iter()
        .find(|item| item.from == from && item.event == event)
        .map(|item| item.to)
        .ok_or_else(|| {
            msg(
                "native.invalidTransition",
                &[
                    ("from", format!("{from:?}").into()),
                    ("event", format!("{event:?}").into()),
                ],
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_contract() {
        let raw = include_str!("../../../contracts/connection-state.vectors.json");
        let parsed: Vectors = serde_json::from_str(raw).unwrap();
        for item in parsed.transitions {
            assert_eq!(reduce(item.from, item.event).unwrap(), item.to);
        }
    }

    #[test]
    fn rejects_unknown_transition() {
        assert!(reduce(Phase::Idle, ConnEvent::CoreReady).is_err());
    }
}
