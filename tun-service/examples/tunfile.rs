// 用给定配置文件启动 TUN（对照实验用）：tunfile <config.json> <保持秒数>
use airport_tun_service::{random_id, Action, Credentials, Timeouts};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cfg_path = args.get(1).expect("config path");
    let hold: u64 = args.get(2).map(|s| s.parse().unwrap_or(30)).unwrap_or(30);
    let home = std::env::var("HOME").expect("HOME");
    let cred: Credentials = serde_json::from_slice(
        &std::fs::read(format!(
            "{home}/Library/Application Support/dev.airport.client/tun-service.json"
        ))
        .expect("read creds"),
    )
    .expect("parse creds");

    let config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cfg_path).expect("read config"))
            .expect("parse config");
    let session = random_id().expect("session");
    println!("session={session} hold={hold}s");
    match cred.exchange(
        Action::Start {
            config,
            session: session.clone(),
        },
        Timeouts::CONTROL,
    ) {
        Ok(resp) => {
            println!("START running={} error={:?}", resp.running, resp.error);
            for line in resp.logs {
                println!("  {line}");
            }
            std::thread::sleep(std::time::Duration::from_secs(hold));
            let _ = cred.exchange(
                Action::Stop {
                    session: session.clone(),
                },
                Timeouts::CONTROL,
            );
            println!("stopped");
        }
        Err(e) => println!("START FAILED: {e}"),
    }
}
