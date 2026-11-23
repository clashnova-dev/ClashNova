//! 本地 SQLite 存储（LAN-008）：传输历史、可信设备、设置。
//!
//! 仅本地保存，不上传。骨架阶段建表并提供连接句柄；增删查方法在实现阶段补齐。

use crate::error::Result;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

/// 表结构。使用 `IF NOT EXISTS` 以便重复打开。
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS transfer_history (
    id            TEXT PRIMARY KEY,
    direction     TEXT NOT NULL,
    peer_alias    TEXT NOT NULL,
    peer_fingerprint TEXT NOT NULL,
    file_name     TEXT NOT NULL,
    size          INTEGER NOT NULL,
    state         TEXT NOT NULL,
    started_at    INTEGER,
    finished_at   INTEGER,
    avg_speed_bps INTEGER,
    save_path     TEXT
);

CREATE TABLE IF NOT EXISTS text_messages (
    id TEXT NOT NULL,
    direction TEXT NOT NULL,
    peer_alias TEXT NOT NULL,
    peer_fingerprint TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (id, direction)
);

CREATE TABLE IF NOT EXISTS trusted_devices (
    fingerprint TEXT PRIMARY KEY,
    alias       TEXT NOT NULL,
    platform    TEXT,
    first_seen  INTEGER,
    last_seen   INTEGER,
    auto_accept INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS lan_settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

/// 一条传输历史记录（对外可序列化，字段对齐前端 camelCase）。
///
/// 会话按 `peer_fingerprint` 分线（微信文件助手式）；`finished_at` 兼作时间线时间戳
/// （文本消息以其 created_at 填入）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRow {
    pub id: String,
    pub direction: String,
    pub peer_alias: String,
    #[serde(default)]
    pub peer_fingerprint: String,
    pub file_name: String,
    pub size: u64,
    pub state: String,
    pub finished_at: Option<i64>,
    pub save_path: Option<String>,
    pub text: Option<String>,
}

/// 一条可信设备记录（TOFU 配对）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedRow {
    pub fingerprint: String,
    pub alias: String,
    pub platform: Option<String>,
    pub auto_accept: bool,
}

/// 把一行查询结果映射为 [`HistoryRow`]（列顺序见 list_history / list_conversation）。
fn map_history_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryRow> {
    Ok(HistoryRow {
        id: r.get(0)?,
        direction: r.get(1)?,
        peer_alias: r.get(2)?,
        peer_fingerprint: r.get(3)?,
        file_name: r.get(4)?,
        size: r.get::<_, i64>(5)? as u64,
        state: r.get(6)?,
        finished_at: r.get(7)?,
        save_path: r.get(8)?,
        text: r.get(9)?,
    })
}

/// 本地存储句柄。连接置于 `Mutex` 内，以便在异步任务间共享。
pub struct Storage {
    conn: Mutex<Connection>,
}

impl Storage {
    /// 打开（或创建）数据库文件并建表。
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 内存库，仅用于测试。
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 暂供后续方法使用的连接访问入口。
    pub fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let guard = self.conn.lock().expect("lan storage mutex poisoned");
        f(&guard)
    }

    /// 写入（或覆盖）一条历史记录。
    pub fn insert_history(&self, row: &HistoryRow) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO transfer_history \
                 (id, direction, peer_alias, peer_fingerprint, file_name, size, state, finished_at, save_path) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    row.id,
                    row.direction,
                    row.peer_alias,
                    row.peer_fingerprint,
                    row.file_name,
                    row.size as i64,
                    row.state,
                    row.finished_at,
                    row.save_path,
                ],
            )?;
            Ok(())
        })
    }

    /// 读取历史记录（按结束时间倒序）。
    pub fn list_history(&self, limit: u32) -> Result<Vec<HistoryRow>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, direction, peer_alias, peer_fingerprint, file_name, size, state, finished_at, save_path, NULL AS text \
                 FROM transfer_history UNION ALL \
                 SELECT id, direction, peer_alias, peer_fingerprint, '', length(CAST(body AS BLOB)), 'COMPLETED', created_at, NULL, body \
                 FROM text_messages ORDER BY finished_at DESC, id DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map([limit], map_history_row)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    /// 读取与某设备（指纹）的会话时间线（文本+文件合并，按时间正序，最旧在前）。
    pub fn list_conversation(&self, fingerprint: &str, limit: u32) -> Result<Vec<HistoryRow>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM ( \
                 SELECT id, direction, peer_alias, peer_fingerprint, file_name, size, state, finished_at, save_path, NULL AS text \
                 FROM transfer_history WHERE peer_fingerprint = ?1 UNION ALL \
                 SELECT id, direction, peer_alias, peer_fingerprint, '', length(CAST(body AS BLOB)), 'COMPLETED', created_at, NULL, body \
                 FROM text_messages WHERE peer_fingerprint = ?1 \
                 ORDER BY finished_at DESC, id DESC LIMIT ?2 \
                 ) ORDER BY finished_at ASC, id ASC",
            )?;
            let rows = stmt.query_map(rusqlite::params![fingerprint, limit], map_history_row)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    pub fn receive_directory(&self) -> Result<Option<String>> {
        use rusqlite::OptionalExtension;
        self.with_conn(|conn| {
            Ok(conn
                .query_row(
                    "SELECT value FROM lan_settings WHERE key='receive_directory'",
                    [],
                    |row| row.get(0),
                )
                .optional()?)
        })
    }

    pub fn save_receive_directory(&self, directory: &str) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute("INSERT INTO lan_settings (key,value) VALUES ('receive_directory',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [directory])?;
            Ok(())
        })
    }

    /// 读取一项通用设置（lan_settings 键值表）。
    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        use rusqlite::OptionalExtension;
        self.with_conn(|conn| {
            Ok(conn
                .query_row("SELECT value FROM lan_settings WHERE key=?1", [key], |row| {
                    row.get(0)
                })
                .optional()?)
        })
    }

    /// 写入一项通用设置（lan_settings 键值表）。
    pub fn save_setting(&self, key: &str, value: &str) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO lan_settings (key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                rusqlite::params![key, value],
            )?;
            Ok(())
        })
    }

    /// Native file actions are restricted to exact paths recorded by file transfers.
    pub fn contains_file_path(&self, path: &str) -> Result<bool> {
        self.with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM transfer_history WHERE save_path=?1)",
                [path],
                |row| row.get(0),
            )?)
        })
    }

    /// 删除一条历史记录（文件或文本），按 id 匹配两张表。返回被删行数。
    pub fn remove_history(&self, id: &str) -> Result<usize> {
        self.with_conn(|conn| {
            let a = conn.execute("DELETE FROM transfer_history WHERE id = ?1", [id])?;
            let b = conn.execute("DELETE FROM text_messages WHERE id = ?1", [id])?;
            Ok(a + b)
        })
    }

    pub fn text_matches(&self, id: &str, peer: &str, text: &str) -> Result<Option<bool>> {
        use rusqlite::OptionalExtension;
        self.with_conn(|conn| Ok(conn.query_row(
            "SELECT peer_fingerprint, body FROM text_messages WHERE id=?1 AND direction='receive'", [id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        ).optional()?.map(|(fp, body)| fp == peer && body == text)))
    }

    pub fn insert_text(&self, row: &HistoryRow, fingerprint: &str) -> Result<bool> {
        self.with_conn(|conn| {
            let n = conn.execute("INSERT OR IGNORE INTO text_messages (id,direction,peer_alias,peer_fingerprint,body,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
                rusqlite::params![row.id,row.direction,row.peer_alias,fingerprint,row.text.as_deref().unwrap_or_default(),row.finished_at.unwrap_or(0)])?;
            Ok(n == 1)
        })
    }

    /// 清空历史记录。
    pub fn clear_history(&self) -> Result<()> {
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            tx.execute("DELETE FROM transfer_history", [])?;
            tx.execute("DELETE FROM text_messages", [])?;
            tx.commit()?;
            Ok(())
        })
    }

    /// 记住一个可信设备（配对成功后调用）；已存在则更新别名与时间。
    ///
    /// 配对即信任：`auto_accept` 置 1，之后该设备的文件/文本直接接收，不再逐次确认
    /// （用户约定“配对过就默认信任”）。
    pub fn upsert_trusted(
        &self,
        fingerprint: &str,
        alias: &str,
        platform: Option<&str>,
    ) -> Result<()> {
        self.with_conn(|conn| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            conn.execute(
                "INSERT INTO trusted_devices (fingerprint, alias, platform, first_seen, last_seen, auto_accept) \
                 VALUES (?1, ?2, ?3, ?4, ?4, 1) \
                 ON CONFLICT(fingerprint) DO UPDATE SET alias = ?2, platform = ?3, last_seen = ?4, auto_accept = 1",
                rusqlite::params![fingerprint, alias, platform, now],
            )?;
            Ok(())
        })
    }

    /// 该指纹是否为可信设备。
    pub fn is_trusted(&self, fingerprint: &str) -> Result<bool> {
        self.with_conn(|conn| {
            let n: i64 = conn.query_row(
                "SELECT count(*) FROM trusted_devices WHERE fingerprint = ?1",
                [fingerprint],
                |r| r.get(0),
            )?;
            Ok(n > 0)
        })
    }

    /// 所有可信设备（最近在前）。
    pub fn list_trusted(&self) -> Result<Vec<TrustedRow>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT fingerprint, alias, platform, auto_accept FROM trusted_devices ORDER BY last_seen DESC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(TrustedRow {
                    fingerprint: r.get(0)?,
                    alias: r.get(1)?,
                    platform: r.get(2)?,
                    auto_accept: r.get::<_, i64>(3)? != 0,
                })
            })?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    /// 移除一个可信设备（解除配对）。
    pub fn remove_trusted(&self, fingerprint: &str) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM trusted_devices WHERE fingerprint = ?1",
                [fingerprint],
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Storage;

    #[test]
    fn in_memory_schema_builds() {
        let storage = Storage::in_memory().expect("open in-memory db");
        // 确认三张表都已创建。
        let count = storage
            .with_conn(|conn| {
                let n: i64 = conn.query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' \
                     AND name IN ('transfer_history','trusted_devices','lan_settings')",
                    [],
                    |row| row.get(0),
                )?;
                Ok(n)
            })
            .expect("schema query");
        assert_eq!(count, 3);
    }
}
