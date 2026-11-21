//! Text shown to the user is sent to the UI as a dictionary key, never as prose: a bare key such
//! as `"native.subscriptionUrlInvalid"`, or `msg(key, &[(name, value)])` when there are values.
//! Keys live in `src/i18n/zh-CN.ts` and `en.ts`; `tests/i18n.test.ts` checks every key used here.
//! The encoder is shared with the TUN service, whose errors reach the same UI.
pub use airport_tun_service::message::{msg, text};
