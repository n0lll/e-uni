//! Snapshot files, SQLite metadata, and typed state diffs (ch. 41.1–41.2).

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use universe_types::{
    diff_states, hex_hash, sha256, state_hash, Hash32, LifeEvent, LifeEventKind,
    RuleSpec, SimError, WorldState,
};

pub fn first_diff(a: &WorldState, b: &WorldState) -> Option<String> {
    diff_states(a, b)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunProgress {
    pub live_tick: u64,
    pub durable_tick: u64,
    pub analysis_tick: u64,
    pub narrated_tick: u64,
}

pub struct Archive {
    pub dir: PathBuf,
    db: Connection,
    pending_bytes: u64,
    max_unwritten: u64,
    hash_every: u64,
    snap_every: u64,
    last_batch_tick: u64,
    /// Test hook: fail after tmp write, before rename/SQLite register.
    pub fail_before_rename: bool,
}

impl Archive {
    pub fn open(dir: impl AsRef<Path>, spec: &RuleSpec) -> Result<Self, SimError> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir).map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        let db_path = dir.join("archive.sqlite");
        let db = Connection::open(&db_path).map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        db.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS batches (
                end_tick INTEGER PRIMARY KEY,
                state_hash BLOB NOT NULL,
                event_count INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS snapshots (
                tick INTEGER PRIMARY KEY,
                filename TEXT NOT NULL,
                bytes INTEGER NOT NULL,
                content_hash BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tick INTEGER NOT NULL,
                kind INTEGER NOT NULL,
                a INTEGER NOT NULL,
                b INTEGER NOT NULL,
                amount INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS hashes (
                tick INTEGER PRIMARY KEY,
                state_hash BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS narratives (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                window_a INTEGER NOT NULL,
                window_b INTEGER NOT NULL,
                predecessor INTEGER,
                body TEXT NOT NULL,
                facts_hash BLOB NOT NULL,
                mode TEXT NOT NULL
            );
            ",
        )
        .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        let rules = spec.encode_canonical().map_err(SimError::from)?;
        db.execute(
            "INSERT OR REPLACE INTO meta(k,v) VALUES ('rules_canonical', ?1)",
            params![rules],
        )
        .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        Ok(Self {
            dir,
            db,
            pending_bytes: 0,
            max_unwritten: spec.safety.max_unwritten_archive_bytes,
            hash_every: spec.archive.state_hash_every_ticks.max(1),
            snap_every: spec.archive.checkpoint_every_ticks.max(1),
            last_batch_tick: 0,
            fail_before_rename: false,
        })
    }

    pub fn progress(&self) -> Result<RunProgress, SimError> {
        Ok(RunProgress {
            live_tick: self.get_u64("live_tick")?,
            durable_tick: self.get_u64("durable_tick")?,
            analysis_tick: self.get_u64("analysis_tick")?,
            narrated_tick: self.get_u64("narrated_tick")?,
        })
    }

    fn get_u64(&self, k: &str) -> Result<u64, SimError> {
        let v: Option<Vec<u8>> = self
            .db
            .query_row("SELECT v FROM meta WHERE k=?1", params![k], |r| r.get(0))
            .optional()
            .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        match v {
            None => Ok(0),
            Some(b) if b.len() == 8 => {
                let mut a = [0u8; 8];
                a.copy_from_slice(&b);
                Ok(u64::from_le_bytes(a))
            }
            _ => Err(SimError::ArchiveFailure("bad meta u64".into())),
        }
    }

    fn set_u64(&self, k: &str, v: u64) -> Result<(), SimError> {
        self.db
            .execute(
                "INSERT OR REPLACE INTO meta(k,v) VALUES (?1, ?2)",
                params![k, v.to_le_bytes().as_slice()],
            )
            .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        Ok(())
    }

    pub fn set_narrated_tick(&self, t: u64) -> Result<(), SimError> {
        self.set_u64("narrated_tick", t)
    }

    pub fn set_analysis_tick(&self, t: u64) -> Result<(), SimError> {
        self.set_u64("analysis_tick", t)
    }

    pub fn set_durable_tick(&self, t: u64) -> Result<(), SimError> {
        self.set_u64("durable_tick", t)
    }

    pub fn pending_bytes(&self) -> u64 {
        self.pending_bytes
    }

    /// Record a committed world boundary. `state.tick` is the *next* tick index (after step).
    pub fn record_committed(
        &mut self,
        state: &WorldState,
        events: &[LifeEvent],
    ) -> Result<(), SimError> {
        let live = state.tick;
        self.set_u64("live_tick", live)?;
        for e in events {
            let row_bytes = 40u64;
            if self.pending_bytes.saturating_add(row_bytes) > self.max_unwritten {
                return Err(SimError::SafetyPause(
                    "unwritten archive buffer exceeds 256 MiB".into(),
                ));
            }
            self.db
                .execute(
                    "INSERT INTO events(tick,kind,a,b,amount) VALUES (?1,?2,?3,?4,?5)",
                    params![
                        e.tick as i64,
                        e.kind.code() as i64,
                        e.entity_a as i64,
                        e.entity_b as i64,
                        e.amount as i64
                    ],
                )
                .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
            self.pending_bytes += row_bytes;
        }
        if live > 0 && live % self.hash_every == 0 {
            self.close_batch(state, events.len())?;
        }
        if live > 0 && live % self.snap_every == 0 {
            self.write_snapshot(state)?;
        }
        Ok(())
    }

    pub fn flush_exit(&mut self, state: &WorldState) -> Result<(), SimError> {
        self.close_batch(state, 0)?;
        Ok(())
    }

    fn close_batch(&mut self, state: &WorldState, extra_events: usize) -> Result<(), SimError> {
        let h = state_hash(state).map_err(SimError::from)?;
        let tick = state.tick;
        let tx = self
            .db
            .unchecked_transaction()
            .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        tx.execute(
            "INSERT OR REPLACE INTO hashes(tick, state_hash) VALUES (?1, ?2)",
            params![tick as i64, h.as_slice()],
        )
        .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        tx.execute(
            "INSERT OR REPLACE INTO batches(end_tick, state_hash, event_count) VALUES (?1,?2,?3)",
            params![tick as i64, h.as_slice(), extra_events as i64],
        )
        .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        tx.execute(
            "INSERT OR REPLACE INTO meta(k,v) VALUES ('durable_tick', ?1)",
            params![tick.to_le_bytes().as_slice()],
        )
        .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        tx.execute(
            "INSERT OR REPLACE INTO meta(k,v) VALUES ('analysis_tick', ?1)",
            params![tick.to_le_bytes().as_slice()],
        )
        .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        tx.commit()
            .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        self.last_batch_tick = tick;
        self.pending_bytes = 0;
        Ok(())
    }

    fn write_snapshot(&mut self, state: &WorldState) -> Result<(), SimError> {
        let bytes = state.encode_canonical().map_err(SimError::from)?;
        let content = sha256(&bytes);
        let tmp = self.dir.join(format!("snap_{}.tmp", state.tick));
        {
            let mut f = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&tmp)
                .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
            f.write_all(&bytes)
                .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
            f.sync_all()
                .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        }
        if self.fail_before_rename {
            let _ = fs::remove_file(&tmp);
            return Err(SimError::ArchiveFailure(
                "injected failure before snapshot rename".into(),
            ));
        }
        let name = format!("snap_{}.eus1", hex_hash(&content));
        let final_path = self.dir.join(&name);
        fs::rename(&tmp, &final_path).map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        if let Ok(dirf) = File::open(&self.dir) {
            let _ = dirf.sync_all();
        }
        let tx = self
            .db
            .unchecked_transaction()
            .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        tx.execute(
            "INSERT OR REPLACE INTO snapshots(tick, filename, bytes, content_hash) VALUES (?1,?2,?3,?4)",
            params![
                state.tick as i64,
                name,
                bytes.len() as i64,
                content.as_slice()
            ],
        )
        .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        tx.commit()
            .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        Ok(())
    }

    pub fn latest_snapshot_tick(&self, not_after: u64) -> Result<Option<(u64, PathBuf, Hash32)>, SimError> {
        let row = self
            .db
            .query_row(
                "SELECT tick, filename, content_hash FROM snapshots WHERE tick <= ?1 ORDER BY tick DESC LIMIT 1",
                params![not_after as i64],
                |r| {
                    let tick: i64 = r.get(0)?;
                    let name: String = r.get(1)?;
                    let hash: Vec<u8> = r.get(2)?;
                    Ok((tick as u64, name, hash))
                },
            )
            .optional()
            .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        match row {
            None => Ok(None),
            Some((tick, name, hash)) => {
                let mut h = [0u8; 32];
                if hash.len() != 32 {
                    return Err(SimError::ArchiveFailure("snapshot hash len".into()));
                }
                h.copy_from_slice(&hash);
                Ok(Some((tick, self.dir.join(name), h)))
            }
        }
    }

    pub fn load_snapshot_file(path: &Path, expect: Option<Hash32>) -> Result<WorldState, SimError> {
        let mut f = File::open(path).map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes)
            .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        let got = sha256(&bytes);
        if let Some(exp) = expect {
            if got != exp {
                return Err(SimError::ArchiveFailure("snapshot content hash mismatch".into()));
            }
        }
        WorldState::decode_canonical(&bytes).map_err(SimError::from)
    }

    pub fn recorded_hash(&self, tick: u64) -> Result<Option<Hash32>, SimError> {
        let v: Option<Vec<u8>> = self
            .db
            .query_row(
                "SELECT state_hash FROM hashes WHERE tick=?1",
                params![tick as i64],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        match v {
            None => Ok(None),
            Some(b) if b.len() == 32 => {
                let mut a = [0u8; 32];
                a.copy_from_slice(&b);
                Ok(Some(a))
            }
            _ => Err(SimError::ArchiveFailure("hash blob".into())),
        }
    }

    pub fn events_in_window(&self, a: u64, b: u64) -> Result<Vec<LifeEvent>, SimError> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT tick, kind, a, b, amount FROM events WHERE tick >= ?1 AND tick < ?2 ORDER BY id",
            )
            .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        let rows = stmt
            .query_map(params![a as i64, b as i64], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })
            .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        let mut out = Vec::new();
        for row in rows {
            let (tick, kind, ea, eb, amt) =
                row.map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
            let kind = LifeEventKind::from_code(kind as u8)
                .ok_or_else(|| SimError::ArchiveFailure("event kind".into()))?;
            out.push(LifeEvent {
                tick: tick as u64,
                kind,
                entity_a: ea as u64,
                entity_b: eb as u64,
                amount: amt as u64,
            });
        }
        Ok(out)
    }

    pub fn insert_narrative(
        &self,
        window_a: u64,
        window_b: u64,
        predecessor: Option<i64>,
        body: &str,
        facts_hash: Hash32,
        mode: &str,
    ) -> Result<i64, SimError> {
        self.db
            .execute(
                "INSERT INTO narratives(window_a,window_b,predecessor,body,facts_hash,mode) VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    window_a as i64,
                    window_b as i64,
                    predecessor,
                    body,
                    facts_hash.as_slice(),
                    mode
                ],
            )
            .map_err(|e| SimError::ArchiveFailure(e.to_string()))?;
        Ok(self.db.last_insert_rowid())
    }

    pub fn latest_narrative_body(&self) -> Result<Option<String>, SimError> {
        self.db
            .query_row(
                "SELECT body FROM narratives ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| SimError::ArchiveFailure(e.to_string()))
    }
}

/// Replay from the latest snapshot not after `durable_tick`, checking stored hashes.
pub fn restore_and_replay<F>(
    archive: &Archive,
    durable_tick: u64,
    mut make_world: F,
) -> Result<WorldState, SimError>
where
    F: FnMut(WorldState) -> Result<WorldState, SimError>,
{
    let snap = archive
        .latest_snapshot_tick(durable_tick)?
        .ok_or_else(|| SimError::ArchiveFailure("no snapshot at or before durable_tick".into()))?;
    let (snap_tick, path, h) = snap;
    let mut state = Archive::load_snapshot_file(&path, Some(h))?;
    if state.tick != snap_tick {
        return Err(SimError::ArchiveFailure(format!(
            "snapshot tick {} vs filename {}",
            state.tick, snap_tick
        )));
    }
    while state.tick < durable_tick {
        state = make_world(state)?;
        if state.tick % archive.hash_every == 0 {
            if let Some(expect) = archive.recorded_hash(state.tick)? {
                let got = state_hash(&state).map_err(SimError::from)?;
                if got != expect {
                    let msg = format!(
                        "hash divergence at tick {} (first field requires a second state)",
                        state.tick
                    );
                    return Err(SimError::ArchiveFailure(msg));
                }
            }
        }
    }
    Ok(state)
}

pub fn divergence_path(expected: &WorldState, got: &WorldState) -> String {
    first_diff(expected, got).unwrap_or_else(|| "state_hash".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use universe_types::RuleSpec;

    #[test]
    fn snapshot_roundtrip_hash() {
        let spec = RuleSpec::eu_core_1();
        let dir = std::env::temp_dir().join(format!(
            "eu-arch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut arch = Archive::open(&dir, &spec).unwrap();
        let st = WorldState::empty(&spec, 1).unwrap();
        arch.write_snapshot(&st).unwrap();
        let (tick, path, h) = arch.latest_snapshot_tick(0).unwrap().unwrap();
        assert_eq!(tick, 0);
        let st2 = Archive::load_snapshot_file(&path, Some(h)).unwrap();
        assert_eq!(state_hash(&st).unwrap(), state_hash(&st2).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn first_diff_tick() {
        let spec = RuleSpec::eu_core_1();
        let mut a = WorldState::empty(&spec, 1).unwrap();
        let b = a.clone();
        a.tick = 3;
        assert_eq!(first_diff(&a, &b).as_deref(), Some("tick"));
    }

    #[test]
    fn rename_failure_does_not_register_snapshot() {
        let spec = RuleSpec::eu_core_1();
        let dir = std::env::temp_dir().join(format!(
            "eu-arch-fail-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut arch = Archive::open(&dir, &spec).unwrap();
        arch.fail_before_rename = true;
        let st = WorldState::empty(&spec, 1).unwrap();
        let err = arch.write_snapshot(&st).unwrap_err();
        match err {
            SimError::ArchiveFailure(msg) => assert!(msg.contains("before snapshot rename")),
            other => panic!("{other:?}"),
        }
        assert!(arch.latest_snapshot_tick(u64::MAX).unwrap().is_none());
        let n: i64 = arch
            .db
            .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
