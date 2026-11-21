// 复现 app 的 TUN 启动，打印服务端拒绝原因原文
use airport_tun_service::{random_id, Action, Credentials, Timeouts, TUN_PREFIX_V4, TUN_PREFIX_V6};
use serde_json::json;

fn main() {
    let home = std::env::var("HOME").expect("HOME");
    let path = format!("{home}/Library/Application Support/dev.airport.client/tun-service.json");
    let cred: Credentials =
        serde_json::from_slice(&std::fs::read(&path).expect("read creds")).expect("parse creds");

    let config = json!({
        "log": {"level": "warn"},
        "inbounds": [
            {"type": "mixed", "listen": "127.0.0.1", "listen_port": 29876},
            {
                "type": "tun", "tag": "tun-in",
                "address": [TUN_PREFIX_V4, TUN_PREFIX_V6],
                "mtu": 9000, "stack": "mixed", "auto_route": true,
                "strict_route": false, "dns_mode": "hijack",
                "route_exclude_address": ["192.168.0.0/16"]
            }
        ],
        "outbounds": [
            {"type": "direct", "tag": "direct"},
            {"type": "selector", "tag": "select", "outbounds": ["direct"]}
        ],
        "route": {"final": "select", "auto_detect_interface": true},
        "experimental": {"clash_api": {"external_controller": "127.0.0.1:57891", "secret": "0123456789abcdef0123456789abcdef01234567"}}
    });

    let session = random_id().expect("session id");
    println!("session={session}");
    match cred.exchange(
        Action::Start {
            config,
            session: session.clone(),
        },
        Timeouts::CONTROL,
    ) {
        Ok(resp) => {
            println!("START ok: running={} error={:?}", resp.running, resp.error);
            for line in resp.logs {
                println!("  log: {line}");
            }
            println!("(保持运行 8 秒供观察)");
            std::thread::sleep(std::time::Duration::from_secs(8));
            let st = cred
                .exchange(Action::Status, Timeouts::CONTROL)
                .expect("status");
            println!(
                "status after start: running={} session={:?} error={:?}",
                st.running, st.session, st.error
            );
            if let Ok(resp) = cred.exchange(
                Action::Heartbeat {
                    session: session.clone(),
                },
                Timeouts::HEARTBEAT,
            ) {
                println!("heartbeat logs ({}):", resp.logs.len());
                for line in resp.logs {
                    println!("  {line}");
                }
            }
            let _ = cred.exchange(Action::Stop { session }, Timeouts::CONTROL);
            println!("stopped");
        }
        Err(e) => println!("START FAILED: {e}"),
    }
}
