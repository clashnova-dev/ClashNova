//! Verify HTTPS through the new local proxy. One unhealthy probe must not hold up a good one.
use std::{future::Future, time::Duration};
const EGRESS_TIMEOUT: Duration = Duration::from_secs(8);

async fn first_success<A, B>(first: A, second: B) -> Result<(), String>
where
    A: Future<Output = Result<(), String>>,
    B: Future<Output = Result<(), String>>,
{
    tokio::pin!(first, second);
    tokio::select! {
        result = &mut first => match result { Ok(()) => Ok(()), Err(a) => second.await.map_err(|b| format!("{a}; {b}")) },
        result = &mut second => match result { Ok(()) => Ok(()), Err(b) => first.await.map_err(|a| format!("{a}; {b}")) },
    }
}

pub fn check_proxy_egress(port: u16) -> Result<(), String> {
    let proxy =
        reqwest::Proxy::all(format!("http://127.0.0.1:{port}")).map_err(|e| e.to_string())?;
    // Every request goes through this exact local proxy; never fall back to the host network.
    let client = reqwest::Client::builder()
        .proxy(proxy)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(EGRESS_TIMEOUT)
        .pool_max_idle_per_host(0)
        .build()
        .map_err(|e| e.to_string())?;
    tauri::async_runtime::block_on(async {
        let check = |url| {
            let client = client.clone();
            async move {
                let response = client.get(url).send().await.map_err(|error| {
                    let mut detail = error.to_string();
                    let mut cause = std::error::Error::source(&error);
                    while let Some(next) = cause {
                        detail.push_str(": ");
                        detail.push_str(&next.to_string());
                        cause = next.source();
                    }
                    detail
                })?;
                if response.status().is_success() {
                    Ok(())
                } else {
                    Err(crate::message::msg(
                        "native.probeHttpStatus",
                        &[("status", crate::message::text(response.status()))],
                    ))
                }
            }
        };
        tokio::time::timeout(
            EGRESS_TIMEOUT,
            first_success(
                check("https://cp.cloudflare.com/generate_204"),
                check("https://www.gstatic.com/generate_204"),
            ),
        )
        .await
        .map_err(|_| "native.proxyEgressTimeout".to_owned())?
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::Instant,
    };
    struct Cancelled(Arc<AtomicBool>);
    impl Drop for Cancelled {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    #[test]
    fn a_slow_probe_is_cancelled_when_the_other_succeeds() {
        tauri::async_runtime::block_on(async {
            let cancelled = Arc::new(AtomicBool::new(false));
            let guard = Cancelled(cancelled.clone());
            let stalled = async move {
                let _guard = guard;
                std::future::pending::<Result<(), String>>().await
            };
            let ready = async {
                tokio::time::sleep(Duration::from_millis(20)).await;
                Ok(())
            };
            let started = Instant::now();
            tokio::time::timeout(Duration::from_millis(300), first_success(stalled, ready))
                .await
                .unwrap()
                .unwrap();
            assert!(started.elapsed() < Duration::from_millis(300));
            assert!(cancelled.load(Ordering::SeqCst));
        });
    }
    #[test]
    fn an_early_failure_waits_for_the_other_probe_but_two_failures_keep_both_causes() {
        tauri::async_runtime::block_on(async {
            first_success(async { Err("first failed".into()) }, async { Ok(()) })
                .await
                .unwrap();
            let error = first_success(async { Err("first failed".into()) }, async {
                Err("second failed".into())
            })
            .await
            .unwrap_err();
            assert!(error.contains("first failed") && error.contains("second failed"));
        });
    }
    #[test]
    fn a_closed_local_proxy_never_succeeds_via_the_system_network() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        assert!(check_proxy_egress(port).is_err());
    }
    #[test]
    #[ignore = "read-only live HTTPS benchmark; requires AIRPORT_STARTUP_TEST_PROXY"]
    fn benchmark_live_proxy_verification() {
        let port: u16 = std::env::var("AIRPORT_STARTUP_TEST_PROXY")
            .unwrap()
            .parse()
            .unwrap();
        for _ in 0..3 {
            let started = Instant::now();
            let client = reqwest::blocking::Client::builder()
                .proxy(reqwest::Proxy::all(format!("http://127.0.0.1:{port}")).unwrap())
                .timeout(EGRESS_TIMEOUT)
                .build()
                .unwrap();
            let previous = client
                .get("https://www.cloudflare.com/cdn-cgi/trace")
                .send()
                .map(|r| r.status().is_success());
            eprintln!(
                "previous single-probe: {} ms, success={}",
                started.elapsed().as_millis(),
                matches!(previous, Ok(true))
            );
            let started = Instant::now();
            let current = check_proxy_egress(port);
            eprintln!(
                "parallel verification: {} ms, success={}",
                started.elapsed().as_millis(),
                current.is_ok()
            );
            assert!(
                current.is_ok(),
                "explicit loopback proxy failed: {current:?}"
            );
        }
    }
}
