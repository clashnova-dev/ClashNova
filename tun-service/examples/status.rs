// 用与服务端同一套实现（airport-tun-service crate）调 Status，排除我 JS 客户端的实现差异
use airport_tun_service::{Action, Credentials};

fn main() {
    let home = std::env::var("HOME").expect("HOME");
    let path = format!("{home}/Library/Application Support/dev.airport.client/tun-service.json");
    let raw = std::fs::read(&path).expect("read credentials");
    let cred: Credentials = serde_json::from_slice(&raw).expect("parse credentials");
    match cred.exchange(Action::Status, airport_tun_service::Timeouts::CONTROL) {
        Ok(resp) => println!(
            "OK running={} blocking={} session={:?} error={:?}\ncore_sha256={:?}\nhelper_sha256={:?}\nlogs={:?}",
            resp.running, resp.blocking, resp.session, resp.error, resp.core_sha256, resp.helper_sha256, resp.logs
        ),
        Err(e) => println!("EXCHANGE FAILED: {e}"),
    }
}
