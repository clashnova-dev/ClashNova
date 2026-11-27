//! Resource bounds for the bundled parser processing untrusted subscription data.
use std::{
    io::Read,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
pub fn output(
    command: &mut Command,
    timeout: Duration,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    let mut process = Process(
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                crate::message::msg(
                    "native.converterRunFailed",
                    &[("error", crate::message::text(e))],
                )
            })?,
    );
    let stdout = process
        .0
        .stdout
        .take()
        .ok_or("native.subscriptionConvertFailed")?;
    let (send, receive) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take(max_bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = send.send(result);
    });
    let deadline = Instant::now() + timeout;
    let mut bytes = None;
    loop {
        if bytes.is_none() {
            match receive.try_recv() {
                Ok(Ok(value)) if value.len() > max_bytes => {
                    return Err("native.converterOutputTooLarge".into())
                }
                Ok(Ok(value)) => bytes = Some(value),
                Ok(Err(_)) | Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("native.subscriptionConvertFailed".into())
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(status) = process.0.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                return Err("native.subscriptionConvertFailed".into());
            }
            if let Some(bytes) = bytes.take() {
                return Ok(bytes);
            }
        }
        if Instant::now() >= deadline {
            return Err("native.converterTimeout".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires the converter sidecar built by scripts/prepare-sidecars.py"]
    fn bundled_converter_reads_the_example_with_resource_bounds() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let binary = root
            .join("binaries")
            .join(format!("sing-box-sub-{}", env!("AIRPORT_TARGET")));
        let mut command = Command::new(binary);
        command
            .arg(root.join("../testdata/example-clash.yaml"))
            .args(["--only-nodes", "--out", "-"]);
        let bytes = output(&mut command, Duration::from_secs(15), 4 * 1024 * 1024).unwrap();
        let nodes: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert!(!nodes.is_empty());
        airport_tun_service::validate_profile_options(&serde_json::json!({"outbounds": nodes}))
            .unwrap();
    }

    #[test]
    fn converter_output_is_bounded_and_a_stalled_process_is_stopped() {
        let mut small = Command::new("/bin/echo");
        small.arg("untrusted & data");
        assert_eq!(
            output(&mut small, Duration::from_secs(1), 128).unwrap(),
            b"untrusted & data\n"
        );
        let mut excessive = Command::new("/usr/bin/yes");
        excessive.arg("overflow");
        assert!(output(&mut excessive, Duration::from_secs(1), 128).is_err());
        let mut stalled = Command::new("/bin/sleep");
        stalled.arg("30");
        let began = Instant::now();
        assert_eq!(
            output(&mut stalled, Duration::from_millis(50), 128).unwrap_err(),
            "native.converterTimeout"
        );
        assert!(began.elapsed() < Duration::from_secs(2));
    }
}
