//! Open validated URLs through an OS API, never a command interpreter.
pub fn validate(raw: &str) -> Result<String, String> {
    if raw.len() > 8192 || raw.chars().any(char::is_control) {
        return Err("native.openUnsupported".into());
    }
    let url = reqwest::Url::parse(raw).map_err(|_| "native.openUnsupported")?;
    if !matches!(url.scheme(), "https" | "http" | "tg")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("native.openUnsupported".into());
    }
    Ok(url.to_string())
}
pub fn open(raw: &str) -> Result<(), String> {
    let url = validate(raw)?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
        let verb: Vec<u16> = "open\0".encode_utf16().collect();
        let target: Vec<u16> = std::ffi::OsStr::new(&url)
            .encode_wide()
            .chain(Some(0))
            .collect();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if result as isize <= 32 {
            return Err("native.openUnsupported".into());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        #[cfg(target_os = "macos")]
        let mut command = std::process::Command::new("/usr/bin/open");
        #[cfg(not(target_os = "macos"))]
        let mut command = std::process::Command::new("xdg-open");
        command.arg(url).spawn().map(|_| ()).map_err(|error| {
            crate::message::msg(
                "native.openFailed",
                &[("error", crate::message::text(error))],
            )
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_real_web_or_telegram_urls_are_allowed() {
        for bad in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://example.com\ncalc.exe",
            "https://user:pass@example.com",
            "http:",
            "tg:",
            "https://example.com\0",
        ] {
            assert!(validate(bad).is_err(), "{bad}");
        }
        for good in [
            "https://example.com/?a=1&b=2",
            "tg://resolve?domain=example",
            "http://localhost:1420",
        ] {
            assert!(validate(good).is_ok(), "{good}");
        }
    }
    #[test]
    fn shell_metacharacters_stay_url_data() {
        // This string was interpreted by `cmd /C start` when passed as an unquoted argument.
        let value = validate("https://example.com/?x=1&calc.exe").unwrap();
        assert_eq!(
            reqwest::Url::parse(&value).unwrap().query(),
            Some("x=1&calc.exe")
        );
    }
}
