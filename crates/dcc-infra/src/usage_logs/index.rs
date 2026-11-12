//! The incremental index: a small sqlite of its own (a cache, rebuilt
//! when its schema version changes) with each log file's size, mtime,
//! the byte offset read up to and what its earlier lines said, and one
//! row per call and turn. A refresh reads only what was appended since
//! (a file that shrank is read again from the start); an unchanged file
//! is not opened. Rows outlive their files, so usage stays after Claude
//! Code prunes old transcripts.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use sha2::{Digest, Sha256};

use dcc_core::domain::usage_logs::{
    EventKind, Runtime, TokenBuckets, UsageEvent, UsageRow, UsageRows, Via,
};

use super::claude::{self, ClaudeFile};
use super::codex::{self, CodexFile};
use super::sources::{Resolver, Sources};

/// Bump when parsing changes: the index is dropped and built again.
const SCHEMA_VERSION: i32 = 1;

/// Files per transaction, so a long cold index keeps its progress.
const FILES_PER_COMMIT: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("usage index database: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("usage index file: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, IndexError>;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RefreshStats {
    /// Log files found.
    pub files: usize,
    /// Files read (new or grown).
    pub parsed: usize,
    pub bytes_read: u64,
    /// New calls and turns.
    pub added: usize,
    pub elapsed: Duration,
}

struct Known {
    id: i64,
    size: u64,
    mtime_ms: i64,
    offset: u64,
    state: String,
}

pub struct UsageIndex {
    conn: Connection,
}

fn key_id(key: &str) -> i64 {
    let digest = Sha256::digest(key.as_bytes());
    i64::from_le_bytes(digest[..8].try_into().expect("8 bytes"))
}

impl UsageIndex {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Self::with(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::with(Connection::open_in_memory()?)
    }

    fn with(conn: Connection) -> Result<Self> {
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version != SCHEMA_VERSION {
            conn.execute_batch(
                "DROP TABLE IF EXISTS events; DROP TABLE IF EXISTS files;
                 CREATE TABLE files (
                     id INTEGER PRIMARY KEY,
                     path TEXT NOT NULL UNIQUE,
                     runtime TEXT NOT NULL,
                     size INTEGER NOT NULL,
                     mtime_ms INTEGER NOT NULL,
                     offset INTEGER NOT NULL,
                     state TEXT NOT NULL
                 );
                 CREATE TABLE events (
                     id INTEGER PRIMARY KEY,
                     file_id INTEGER NOT NULL,
                     kind INTEGER NOT NULL,
                     at_ms INTEGER NOT NULL,
                     runtime TEXT NOT NULL,
                     model TEXT NOT NULL,
                     via TEXT NOT NULL,
                     project TEXT NOT NULL,
                     agent TEXT NOT NULL,
                     session TEXT NOT NULL,
                     subagent INTEGER NOT NULL,
                     input INTEGER NOT NULL,
                     cache_read INTEGER NOT NULL,
                     cache_write INTEGER NOT NULL,
                     output INTEGER NOT NULL,
                     reasoning INTEGER NOT NULL
                 );
                 CREATE INDEX events_at ON events(at_ms);
                 CREATE INDEX events_file ON events(file_id);",
            )?;
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(Self { conn })
    }

    /// Reads what is new in every log under `sources`; `progress(done,
    /// total)` after each file that had something new.
    pub fn refresh(
        &mut self,
        sources: &Sources,
        resolver: &mut Resolver,
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<RefreshStats> {
        let started = Instant::now();
        let mut files: Vec<(PathBuf, Runtime)> = Vec::new();
        for root in &sources.claude {
            list_jsonl(root, &mut |path| files.push((path, Runtime::Claude)));
        }
        for root in &sources.codex {
            list_jsonl(root, &mut |path| {
                let rollout = path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("rollout-"));
                if rollout {
                    files.push((path, Runtime::Codex));
                }
            });
        }
        let mut known = self.known_files()?;
        let work: Vec<(PathBuf, Runtime, u64, i64)> = files
            .iter()
            .filter_map(|(path, runtime)| {
                let meta = std::fs::metadata(path).ok()?;
                let mtime_ms = meta
                    .modified()
                    .ok()
                    .and_then(|at| at.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |at| at.as_millis() as i64);
                let key = path.to_string_lossy();
                let unchanged = known
                    .get(key.as_ref())
                    .is_some_and(|file| file.size == meta.len() && file.mtime_ms == mtime_ms);
                (!unchanged).then(|| (path.clone(), *runtime, meta.len(), mtime_ms))
            })
            .collect();
        let mut stats = RefreshStats {
            files: files.len(),
            ..Default::default()
        };
        let total = work.len();
        progress(0, total);
        for (chunk_index, chunk) in work.chunks(FILES_PER_COMMIT).enumerate() {
            let tx = self.conn.transaction()?;
            for (offset, (path, runtime, size, mtime_ms)) in chunk.iter().enumerate() {
                let previous = known.remove(path.to_string_lossy().as_ref());
                let (bytes, added) =
                    index_file(&tx, previous, path, *runtime, *size, *mtime_ms, resolver)?;
                stats.parsed += 1;
                stats.bytes_read += bytes;
                stats.added += added;
                progress(chunk_index * FILES_PER_COMMIT + offset + 1, total);
            }
            tx.commit()?;
        }
        stats.elapsed = started.elapsed();
        Ok(stats)
    }

    /// Reads one file's new lines (tests and callers that know the file).
    pub fn index_path(
        &mut self,
        path: &Path,
        runtime: Runtime,
        resolver: &mut Resolver,
    ) -> Result<usize> {
        let meta = std::fs::metadata(path)?;
        let mut known = self.known_files()?;
        let previous = known.remove(path.to_string_lossy().as_ref());
        let tx = self.conn.transaction()?;
        let (_, added) = index_file(&tx, previous, path, runtime, meta.len(), 0, resolver)?;
        tx.commit()?;
        Ok(added)
    }

    fn known_files(&self) -> Result<HashMap<String, Known>> {
        let mut statement = self
            .conn
            .prepare("SELECT id, path, size, mtime_ms, offset, state FROM files")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                Known {
                    id: row.get(0)?,
                    size: row.get::<_, i64>(2)? as u64,
                    mtime_ms: row.get(3)?,
                    offset: row.get::<_, i64>(4)? as u64,
                    state: row.get(5)?,
                },
            ))
        })?;
        Ok(rows.flatten().collect())
    }

    /// Calls and turns since `since_ms` (all when `None`).
    pub fn load_rows(&self, since_ms: Option<i64>) -> Result<UsageRows> {
        let mut statement = self.conn.prepare(
            "SELECT kind, at_ms, runtime, model, via, project, agent, session, subagent,
                    input, cache_read, cache_write, output, reasoning
             FROM events WHERE at_ms >= ?1",
        )?;
        let mut rows = UsageRows::default();
        let mut query = statement.query(params![since_ms.unwrap_or(i64::MIN)])?;
        while let Some(row) = query.next()? {
            let runtime = Runtime::parse(row.get_ref(2)?.as_str().unwrap_or(""));
            let Some(runtime) = runtime else {
                continue;
            };
            let model = rows.intern(row.get_ref(3)?.as_str().unwrap_or(""));
            let via = Via::parse(row.get_ref(4)?.as_str().unwrap_or(""));
            let project = rows.intern(row.get_ref(5)?.as_str().unwrap_or(""));
            let agent = rows.intern(row.get_ref(6)?.as_str().unwrap_or(""));
            let session = rows.intern(row.get_ref(7)?.as_str().unwrap_or(""));
            let count = |index: usize| row.get::<_, i64>(index).map(|value| value.max(0) as u64);
            rows.rows.push(UsageRow {
                at_ms: row.get(1)?,
                kind: if row.get::<_, i64>(0)? == 1 {
                    EventKind::Turn
                } else {
                    EventKind::Call
                },
                runtime,
                via,
                subagent: row.get::<_, i64>(8)? != 0,
                model,
                project,
                agent,
                session,
                tokens: TokenBuckets {
                    input: count(9)?,
                    cache_read: count(10)?,
                    cache_write: count(11)?,
                    output: count(12)?,
                    reasoning: count(13)?,
                },
            });
        }
        Ok(rows)
    }

    /// Rows and files kept.
    pub fn counts(&self) -> Result<(u64, u64, u64)> {
        let calls =
            self.conn
                .query_row("SELECT COUNT(*) FROM events WHERE kind = 0", [], |row| {
                    row.get::<_, i64>(0)
                })?;
        let turns =
            self.conn
                .query_row("SELECT COUNT(*) FROM events WHERE kind = 1", [], |row| {
                    row.get::<_, i64>(0)
                })?;
        let files = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get::<_, i64>(0))?;
        Ok((calls as u64, turns as u64, files as u64))
    }
}

/// Every `*.jsonl` under `root`.
fn list_jsonl(root: &Path, found: &mut dyn FnMut(PathBuf)) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            list_jsonl(&path, found);
        } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "jsonl") {
            found(path);
        }
    }
}

enum FileState {
    Claude(ClaudeFile),
    Codex(CodexFile),
}

impl FileState {
    fn fresh(path: &Path, runtime: Runtime) -> Self {
        match runtime {
            Runtime::Claude => FileState::Claude(ClaudeFile::for_path(path)),
            Runtime::Codex => FileState::Codex(CodexFile::default()),
        }
    }

    fn restore(state: &str, path: &Path, runtime: Runtime) -> Self {
        match runtime {
            Runtime::Claude => serde_json::from_str(state)
                .map(FileState::Claude)
                .unwrap_or_else(|_| Self::fresh(path, runtime)),
            Runtime::Codex => serde_json::from_str(state)
                .map(FileState::Codex)
                .unwrap_or_else(|_| Self::fresh(path, runtime)),
        }
    }

    fn save(&self) -> String {
        match self {
            FileState::Claude(file) => serde_json::to_string(file),
            FileState::Codex(file) => serde_json::to_string(file),
        }
        .unwrap_or_default()
    }

    fn parse(&mut self, line: &str, resolver: &mut Resolver) -> Option<UsageEvent> {
        match self {
            FileState::Claude(file) => claude::parse_line(line, file, resolver),
            FileState::Codex(file) => codex::parse_line(line, file, resolver),
        }
    }
}

/// Reads `path` from where it was left (or from the start when it is new
/// or shrank) up to its last complete line. Returns bytes read and rows
/// added.
fn index_file(
    tx: &Transaction,
    previous: Option<Known>,
    path: &Path,
    runtime: Runtime,
    size: u64,
    mtime_ms: i64,
    resolver: &mut Resolver,
) -> Result<(u64, usize)> {
    let path_text = path.to_string_lossy();
    let (file_id, mut offset, mut state) = match previous {
        Some(known) if size >= known.offset => (
            known.id,
            known.offset,
            FileState::restore(&known.state, path, runtime),
        ),
        Some(known) => {
            // Rewritten: what it said before is gone.
            tx.execute("DELETE FROM events WHERE file_id = ?1", params![known.id])?;
            (known.id, 0, FileState::fresh(path, runtime))
        }
        None => {
            tx.execute(
                "INSERT INTO files (path, runtime, size, mtime_ms, offset, state)
                 VALUES (?1, ?2, 0, 0, 0, '')
                 ON CONFLICT(path) DO NOTHING",
                params![path_text, runtime.as_str()],
            )?;
            let id: i64 = tx
                .query_row(
                    "SELECT id FROM files WHERE path = ?1",
                    params![path_text],
                    |row| row.get(0),
                )
                .optional()?
                .unwrap_or_default();
            (id, 0, FileState::fresh(path, runtime))
        }
    };

    let mut bytes = 0;
    let mut added = 0;
    if size > offset {
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut reader = BufReader::with_capacity(1 << 18, file);
        let mut insert = tx.prepare_cached(
            "INSERT OR IGNORE INTO events (id, file_id, kind, at_ms, runtime, model, via,
                 project, agent, session, subagent, input, cache_read, cache_write, output,
                 reasoning)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        )?;
        let mut line = Vec::with_capacity(1 << 14);
        loop {
            line.clear();
            let read = reader.read_until(b'\n', &mut line)?;
            // The end, or a line still being written: left for next time.
            if read == 0 || line.last() != Some(&b'\n') {
                break;
            }
            offset += read as u64;
            bytes += read as u64;
            let Ok(text) = std::str::from_utf8(&line) else {
                continue;
            };
            let Some(event) = state.parse(text.trim_end(), resolver) else {
                continue;
            };
            added += insert.execute(params![
                key_id(&event.key),
                file_id,
                i64::from(event.kind == EventKind::Turn),
                event.at_ms,
                event.runtime.as_str(),
                event.model,
                event.via.as_str(),
                event.project,
                event.agent,
                event.session,
                i64::from(event.subagent),
                event.tokens.input as i64,
                event.tokens.cache_read as i64,
                event.tokens.cache_write as i64,
                event.tokens.output as i64,
                event.tokens.reasoning as i64,
            ])?;
        }
    }
    tx.execute(
        "UPDATE files SET size = ?2, mtime_ms = ?3, offset = ?4, state = ?5 WHERE id = ?1",
        params![file_id, size as i64, mtime_ms, offset as i64, state.save()],
    )?;
    Ok((bytes, added))
}
