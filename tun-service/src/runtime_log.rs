//! Daily private log files. Keep today and the preceding six local calendar days.
use chrono::{Local, NaiveDate};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
pub type SharedLog = Arc<Mutex<RuntimeLog>>;
const LIMIT: usize = 500;
const LINE_LIMIT: usize = 4096;

pub struct RuntimeLog {
    dir: PathBuf,
    day: Option<NaiveDate>,
    file: Option<File>,
    lines: VecDeque<String>,
}
impl RuntimeLog {
    pub fn open(dir: PathBuf) -> io::Result<SharedLog> {
        fs::create_dir_all(&dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        }
        let today = Local::now().date_naive();
        cleanup(&dir, today)?;
        let mut log = Self {
            dir,
            day: None,
            file: None,
            lines: VecDeque::new(),
        };
        let mut paths: Vec<_> = fs::read_dir(&log.dir)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| log_date(p).is_some())
            .collect();
        paths.sort();
        for path in paths.into_iter().rev() {
            let mut file = File::open(path)?;
            let len = file.metadata()?.len();
            // Bound restart I/O even for a very busy core; discard any partial first line.
            let start = len.saturating_sub(256 * 1024);
            file.seek(SeekFrom::Start(start))?;
            let mut reader = BufReader::new(file);
            if start > 0 {
                let mut discard = Vec::new();
                reader.read_until(b'\n', &mut discard)?;
            }
            let mut tail = VecDeque::new();
            for line in reader.split(b'\n') {
                tail.push_back(String::from_utf8_lossy(&line?).into_owned());
                if tail.len() > LIMIT {
                    tail.pop_front();
                }
            }
            for line in tail.into_iter().rev() {
                log.lines.push_front(line);
                if log.lines.len() >= LIMIT {
                    break;
                }
            }
            if log.lines.len() >= LIMIT {
                break;
            }
        }
        Ok(Arc::new(Mutex::new(log)))
    }
    pub fn recent(&self) -> Vec<String> {
        self.lines.iter().cloned().collect()
    }
    pub fn drain(&mut self) -> Vec<String> {
        self.lines.drain(..).collect()
    }
    pub fn maintain(&mut self) -> io::Result<()> {
        let today = Local::now().date_naive();
        if self.day == Some(today) {
            return Ok(());
        }
        self.file = None;
        cleanup(&self.dir, today)?;
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        self.file = Some(options.open(self.dir.join(format!("runtime-{today}.log")))?);
        self.day = Some(today);
        Ok(())
    }
    pub fn append(&mut self, text: &str) -> io::Result<()> {
        self.maintain()?;
        // Strip control characters; keep each application record on one line.
        let clean: String = text
            .chars()
            .filter(|c| !c.is_control())
            .take(LINE_LIMIT / 4)
            .collect();
        let line = format!("{}  {}", Local::now().format("%Y-%m-%d %H:%M:%S"), clean);
        writeln!(self.file.as_mut().unwrap(), "{line}")?;
        self.lines.push_back(line);
        if self.lines.len() > LIMIT {
            self.lines.pop_front();
        }
        Ok(())
    }
}
fn log_date(path: &Path) -> Option<NaiveDate> {
    let name = path.file_name()?.to_str()?;
    NaiveDate::parse_from_str(
        name.strip_prefix("runtime-")?.strip_suffix(".log")?,
        "%Y-%m-%d",
    )
    .ok()
}
fn cleanup(dir: &Path, today: NaiveDate) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && log_date(&entry.path())
                .is_some_and(|day| today.signed_duration_since(day).num_days() >= 7)
        {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}
pub fn capture(reader: impl Read + Send + 'static, log: SharedLog) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        loop {
            let mut bytes = Vec::new();
            match reader
                .by_ref()
                .take(LINE_LIMIT as u64)
                .read_until(b'\n', &mut bytes)
            {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if let Ok(mut log) = log.lock() {
                        if let Err(error) = log.append(&String::from_utf8_lossy(&bytes)) {
                            eprintln!("Log write failed: {error}");
                        }
                    }
                }
            }
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_unicode_tail_and_idle_cleanup_are_bounded() {
        let dir =
            std::env::temp_dir().join(format!("airport-log-test-{}", crate::random_id().unwrap()));
        let log = RuntimeLog::open(dir.clone()).unwrap();
        for _ in 0..600 {
            // Two CJK characters, three bytes each in UTF-8.
            log.lock()
                .unwrap()
                .append(&"\u{65e5}\u{5fd7}".repeat(600))
                .unwrap();
        }
        drop(log);
        let log = RuntimeLog::open(dir.clone()).unwrap();
        assert!(!log.lock().unwrap().recent().is_empty());
        assert!(log.lock().unwrap().recent().len() <= LIMIT);
        let old = dir.join(format!(
            "runtime-{}.log",
            Local::now().date_naive() - chrono::Duration::days(7)
        ));
        fs::write(&old, "expired").unwrap();
        log.lock().unwrap().maintain().unwrap();
        assert!(!old.exists());
        let bytes = serde_json::to_vec(&log.lock().unwrap().drain()).unwrap();
        assert!(bytes.len() < crate::MAX_FRAME / 2);
        drop(log);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn retention_restarts_and_private_files() {
        let dir =
            std::env::temp_dir().join(format!("airport-log-test-{}", crate::random_id().unwrap()));
        fs::create_dir_all(&dir).unwrap();
        let today = Local::now().date_naive();
        let old = dir.join(format!("runtime-{}.log", today - chrono::Duration::days(7)));
        let keep = dir.join(format!("runtime-{}.log", today - chrono::Duration::days(6)));
        fs::write(&old, "old\n").unwrap();
        fs::write(&keep, "kept\n").unwrap();
        fs::write(dir.join("unrelated.log"), "keep").unwrap();
        let log = RuntimeLog::open(dir.clone()).unwrap();
        assert!(!old.exists());
        assert!(keep.exists());
        assert!(dir.join("unrelated.log").exists());
        log.lock().unwrap().append("new record").unwrap();
        drop(log);
        let reopened = RuntimeLog::open(dir.clone()).unwrap();
        let lines = reopened.lock().unwrap().recent();
        assert_eq!(lines[0], "kept");
        assert!(lines[1].ends_with("new record"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(dir.join(format!("runtime-{today}.log")))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        drop(reopened);
        fs::remove_dir_all(dir).unwrap();
    }
}
