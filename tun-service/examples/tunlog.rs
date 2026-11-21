// 取 TUN 引擎日志：先 Status 拿 session，再 Heartbeat（日志只随 heartbeat 回传）
use airport_tun_service::{Action, Credentials, Timeouts};

fn main() {
    let home = std::env::var("HOME").expect("HOME");
    let path = format!("{home}/Library/Application Support/dev.airport.client/tun-service.json");
    let raw = std::fs::read(&path).expect("read credentials");
    let cred: Credentials = serde_json::from_slice(&raw).expect("parse credentials");

    let st = cred
        .exchange(Action::Status, Timeouts::CONTROL)
        .expect("status");
    println!(
        "status: running={} blocking={} session={:?}",
        st.running, st.blocking, st.session
    );
    if let Some(session) = st.session.clone() {
        match cred.exchange(Action::Heartbeat { session }, Timeouts::HEARTBEAT) {
            Ok(resp) => {
                println!("heartbeat: running={} error={:?}", resp.running, resp.error);
                println!("--- 引擎日志 ({} 条) ---", resp.logs.len());
                for line in resp.logs {
                    println!("  {line}");
                }
            }
            Err(e) => println!("heartbeat failed: {e}"),
        }
    }
}
