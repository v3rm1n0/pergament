//! Automatic backup and folder sync: settings and state, stored next to
//! `userData.db`, and the timing of sync work (as plain code, driven by the
//! worker in `lib.rs`).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pergament::Result;
use pergament::autobackup::{self, ROLLING_DIR};
use pergament::folder_sync::{self, PeerResult, SyncFolder, SyncState};
use pergament::sync_snapshot::DeleteGuard;
use pergament::userdata::{Conflict, UserData};
use serde::{Deserialize, Serialize};

const SETTINGS_FILE: &str = "auto.json";
const STATE_FILE: &str = "auto-state.json";
/// Smallest gap between two scheduled rolling backups.
const DAY_SECS: u64 = 24 * 60 * 60;
const KEEP_MIN: usize = 1;
const KEEP_MAX: usize = 100;
/// Where the sync state and the bases live, under the library root.
const SYNC_STATE_DIR: &str = "sync";
/// The own file is written this long after the last edit.
pub const WRITE_DELAY: Duration = Duration::from_secs(5);
/// Changes in the sync folder are merged this long after the last one.
pub const FOLDER_DELAY: Duration = Duration::from_secs(2);
/// The folder is checked this often even without a watcher event.
pub const POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);
/// The error for a sync folder in the library folder; the UI translates it.
pub const SYNC_IN_LIBRARY: &str =
    "The library folder and its backups folder cannot be the sync folder.";
/// The status keeps this many of the newest conflicts.
pub const MAX_CONFLICTS: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Rolling {
    pub enabled: bool,
    pub keep: usize,
}

impl Default for Rolling {
    fn default() -> Self {
        Self {
            enabled: true,
            keep: 10,
        }
    }
}

/// Folder sync: off while `folder` is `None`. The device id outlives the folder.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncSettings {
    pub folder: Option<PathBuf>,
    pub device_id: Option<String>,
    /// The host part of the own file name, as it was when the id was made,
    /// so a renamed computer keeps its file. Older settings get it on first use.
    pub host: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoSettings {
    pub rolling: Rolling,
    pub sync: SyncSettings,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoState {
    /// Fingerprint of the data in the newest rolling backup.
    pub rolling_fingerprint: Option<String>,
    /// UNIX seconds of the newest rolling backup.
    pub rolling_at: Option<u64>,
}

/// Clamp a requested number of kept backups to the allowed range.
pub fn clamp_keep(keep: usize) -> usize {
    keep.clamp(KEEP_MIN, KEEP_MAX)
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Read a JSON file; a missing file gives the default, a broken one is logged.
fn read_or_default<T: Default + for<'de> Deserialize<'de>>(path: &Path) -> T {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            eprintln!("ignoring broken {}: {e}", path.display());
            T::default()
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => T::default(),
        Err(e) => {
            eprintln!("cannot read {}: {e}", path.display());
            T::default()
        }
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    // Write a sibling file and rename it, so a crash never leaves a truncated file.
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

/// What the sync worker should do now.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Due {
    /// Write the own file.
    pub write: bool,
    /// Merge the other files (and write the own one).
    pub sync: bool,
}

/// When sync work is due. Every method takes the time, so tests choose it.
#[derive(Debug, Default)]
pub struct Scheduler {
    /// When the debounced write of the own file is due.
    write_at: Option<Instant>,
    /// Data changed since the own file was last written (or the write failed).
    unwritten: bool,
    /// When the debounced merge after a folder event is due.
    folder_at: Option<Instant>,
    /// When the folder is checked without an event.
    poll_at: Option<Instant>,
    /// The app is closing: the close path wrote the own file, the worker stops.
    closed: bool,
}

impl Scheduler {
    /// The user data changed: write the own file `WRITE_DELAY` after the last change.
    pub fn changed(&mut self, now: Instant) {
        self.unwritten = true;
        self.write_at = Some(now + WRITE_DELAY);
    }

    /// The watcher saw a device file change.
    pub fn folder_changed(&mut self, now: Instant) {
        self.folder_at = Some(now + FOLDER_DELAY);
    }

    /// The next time `take_due` has something to do.
    pub fn next_deadline(&self) -> Option<Instant> {
        if self.closed {
            return None;
        }
        [self.write_at, self.folder_at, self.poll_at]
            .into_iter()
            .flatten()
            .min()
    }

    /// What is due at `now`; a due timer is cleared.
    pub fn take_due(&mut self, now: Instant) -> Due {
        if self.closed {
            return Due::default();
        }
        let fire = |at: &mut Option<Instant>| {
            let due = at.is_some_and(|t| t <= now);
            if due {
                *at = None;
            }
            due
        };
        let write = fire(&mut self.write_at);
        let folder = fire(&mut self.folder_at);
        let poll = fire(&mut self.poll_at);
        Due {
            write,
            sync: folder || poll,
        }
    }

    /// The own file is about to be written with the data as it is now.
    pub fn begin_write(&mut self) {
        self.unwritten = false;
        self.write_at = None;
    }

    /// Writing the own file failed; the close path tries again.
    pub fn write_failed(&mut self) {
        self.unwritten = true;
    }

    /// A merge round ran (or failed); the next poll is `POLL_INTERVAL` away.
    pub fn synced(&mut self, now: Instant) {
        self.folder_at = None;
        self.poll_at = Some(now + POLL_INTERVAL);
    }

    /// The app closes: stop the worker and tell whether the own file still
    /// needs a write.
    pub fn on_close(&mut self) -> bool {
        self.closed = true;
        self.write_at = None;
        std::mem::take(&mut self.unwritten)
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }
}

/// Whether the sync folder could be read in the last round. A watcher on a
/// folder that disappeared watches nothing once it is back.
#[derive(Debug, Default)]
pub struct FolderHealth {
    failed: bool,
}

impl FolderHealth {
    /// A round could (`readable`) or could not read the folder; true when
    /// the watcher should be created anew, as the folder is back.
    pub fn round(&mut self, readable: bool) -> bool {
        let back = self.failed && readable;
        self.failed = !readable;
        back
    }
}

/// A peer whose deletions wait for "Apply deletions" or "Keep my data".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingPeer {
    pub file: String,
    pub device: String,
    pub deletions: i64,
}

/// What the status line shows about folder sync.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub folder: Option<PathBuf>,
    /// UNIX seconds of the last merge round that could read the folder.
    pub last_sync: Option<u64>,
    /// Device of the last merged file.
    pub last_device: Option<String>,
    pub error: Option<String>,
    pub pending: Vec<PendingPeer>,
    /// From the last merges, the newest `MAX_CONFLICTS`.
    pub conflicts: Vec<Conflict>,
}

impl SyncStatus {
    /// Take in a merge round's results; true when a merge added, updated or
    /// deleted something.
    pub fn record(&mut self, results: Vec<PeerResult>, now: u64) -> bool {
        let mut changed = false;
        let mut failures = Vec::new();
        for result in results {
            match result {
                PeerResult::Merged {
                    file,
                    device,
                    report,
                    deleted,
                } => {
                    self.pending.retain(|p| p.file != file);
                    changed |= report.added > 0 || report.updated > 0 || deleted > 0;
                    self.last_device = Some(device);
                    self.conflicts.extend(report.conflicts);
                }
                PeerResult::NeedsConfirmation {
                    file,
                    device,
                    deletions,
                } => {
                    self.pending.retain(|p| p.file != file);
                    self.pending.push(PendingPeer {
                        file,
                        device,
                        deletions,
                    });
                }
                PeerResult::Failed { file, error } => failures.push(format!("{file}: {error}")),
            }
        }
        let extra = self.conflicts.len().saturating_sub(MAX_CONFLICTS);
        self.conflicts.drain(..extra);
        self.last_sync = Some(now);
        self.error = (!failures.is_empty()).then(|| failures.join("\n"));
        changed
    }

    /// Add an error beside the ones the round reported.
    pub fn add_error(&mut self, error: String) {
        self.error = Some(match self.error.take() {
            Some(earlier) => format!("{earlier}\n{error}"),
            None => error,
        });
    }

    /// Make the pending list match the peers held back in `state`: a choice
    /// still waits after a restart, when the round reports nothing for it.
    pub fn sync_pending(&mut self, state: &SyncState) {
        let waiting: BTreeMap<&String, i64> = state
            .peers
            .iter()
            .filter_map(|(file, peer)| peer.pending.map(|d| (file, d)))
            .collect();
        self.pending.retain(|p| waiting.contains_key(&p.file));
        for (file, deletions) in waiting {
            if !self.pending.iter().any(|p| &p.file == file) {
                self.pending.push(PendingPeer {
                    file: file.clone(),
                    device: device_of_file(file),
                    deletions,
                });
            }
        }
    }
}

/// `Pergament (<host>)` from `Pergament-<host>-<id>.jwlibrary`, as the
/// device's manifest names it.
fn device_of_file(file: &str) -> String {
    let host = file
        .strip_prefix("Pergament-")
        .and_then(|f| f.strip_suffix(".jwlibrary"))
        .and_then(|f| f.rsplit_once('-'))
        .map_or(file, |(host, _id)| host);
    format!("Pergament ({host})")
}

/// A merge round with these choices deletes rows the user chose to delete,
/// so a rolling backup is written first.
pub fn needs_safety_backup(choices: &BTreeMap<String, DeleteGuard>) -> bool {
    choices.values().any(|g| *g == DeleteGuard::Apply)
}

/// Work for the sync worker.
#[derive(Debug)]
pub enum SyncMsg {
    /// Look at the timers again (an edit set one).
    Wake,
    /// A device file in the folder changed.
    FolderChanged,
    /// Write the own file now.
    WriteNow,
    /// The folder setting changed: watch the new folder.
    Reconfigure,
    /// Merge the other files now, with the user's choices for held-back peers;
    /// `done` is dropped when the round is over.
    Sync {
        choices: BTreeMap<String, DeleteGuard>,
        done: Option<Sender<()>>,
    },
}

/// Lock order: the user data mutex (in `AppState`) before any lock here. The
/// locks here are held briefly and never while taking another lock, except
/// that `write_rolling_now`, `write_safety_backup` and `set_sync_folder` run
/// under the user lock.
pub struct Auto {
    root: PathBuf,
    host: String,
    pub settings: Mutex<AutoSettings>,
    pub state: Mutex<AutoState>,
    close_backup_claimed: AtomicBool,
    pub schedule: Mutex<Scheduler>,
    pub status: Mutex<SyncStatus>,
    worker: OnceLock<Sender<SyncMsg>>,
}

impl Auto {
    pub fn load(root: &Path) -> Auto {
        Auto {
            root: root.to_owned(),
            settings: Mutex::new(read_or_default(&root.join(SETTINGS_FILE))),
            state: Mutex::new(read_or_default(&root.join(STATE_FILE))),
            close_backup_claimed: AtomicBool::new(false),
            host: folder_sync::hostname(),
            schedule: Mutex::new(Scheduler::default()),
            status: Mutex::new(SyncStatus::default()),
            worker: OnceLock::new(),
        }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    /// Connect the sync worker; messages before this are dropped.
    pub fn set_worker(&self, tx: Sender<SyncMsg>) {
        let _ = self.worker.set(tx);
    }

    /// Hand work to the sync worker.
    pub fn send(&self, msg: SyncMsg) {
        if let Some(tx) = self.worker.get() {
            let _ = tx.send(msg);
        }
    }

    /// The user data changed: the own file is written a little later.
    pub fn changed(&self) {
        if self.sync_folder().is_none() {
            return;
        }
        if let Ok(mut s) = self.schedule.lock() {
            s.changed(Instant::now());
        }
        self.send(SyncMsg::Wake);
    }

    /// The sync folder as configured; `None` while sync is off.
    pub fn sync_folder(&self) -> Option<SyncFolder> {
        let sync = self.settings().sync;
        let id = sync.device_id?;
        let dir = sync.folder?;
        let host = sync.host.unwrap_or_else(|| self.remember_host());
        Some(SyncFolder {
            dir,
            own: folder_sync::device_file_name(&host, &id),
            state_dir: self.root.join(SYNC_STATE_DIR),
        })
    }

    /// Store this computer's name for the own file in settings that lack
    /// it, and return the stored name.
    fn remember_host(&self) -> String {
        let fresh = folder_sync::file_host(&self.host);
        let Ok(mut settings) = self.settings.lock() else {
            return fresh;
        };
        if let Some(host) = &settings.sync.host {
            return host.clone();
        }
        settings.sync.host = Some(fresh.clone());
        if let Err(e) = write_json(&self.root.join(SETTINGS_FILE), &*settings) {
            eprintln!("cannot save the device name: {e}");
        }
        fresh
    }

    /// The status, with the folder as configured now.
    pub fn sync_status(&self) -> SyncStatus {
        let folder = self.settings().sync.folder;
        let mut status = self.status.lock().map(|s| s.clone()).unwrap_or_default();
        status.folder = folder;
        status
    }

    /// Change the sync folder and save the settings. A new folder gets a
    /// device id if there is none; leaving a folder (stop, or another one)
    /// drops the sync state and the bases, and leaves the folder's files.
    /// Call it under the user data lock, so no merge runs meanwhile.
    pub fn set_sync_folder(&self, folder: Option<PathBuf>) -> std::io::Result<AutoSettings> {
        if folder.as_deref().is_some_and(|f| self.in_library(f)) {
            return Err(std::io::Error::other(SYNC_IN_LIBRARY));
        }
        let mut settings = self
            .settings
            .lock()
            .map_err(|_| std::io::Error::other("settings lock poisoned"))?;
        let mut next = settings.clone();
        let left = next.sync.folder != folder;
        if left {
            SyncFolder {
                dir: PathBuf::new(),
                own: String::new(),
                state_dir: self.root.join(SYNC_STATE_DIR),
            }
            .forget();
        }
        if folder.is_some() && next.sync.device_id.is_none() {
            next.sync.device_id = Some(folder_sync::new_device_id());
        }
        if folder.is_some() && next.sync.host.is_none() {
            next.sync.host = Some(folder_sync::file_host(&self.host));
        }
        next.sync.folder = folder;
        write_json(&self.root.join(SETTINGS_FILE), &next)?;
        *settings = next.clone();
        drop(settings);
        if left && let Ok(mut status) = self.status.lock() {
            *status = SyncStatus::default();
        }
        Ok(next)
    }

    /// `folder` is the library folder or (in) its backups folder: rolling
    /// backups are no device files, and the other computers' files would
    /// mix with them.
    fn in_library(&self, folder: &Path) -> bool {
        let real = |p: &Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_owned());
        let folder = real(folder);
        folder == real(&self.root) || folder.starts_with(real(&self.rolling_dir()))
    }

    /// True for the first caller only: the close backup runs once per process.
    pub fn claim_close_backup(&self) -> bool {
        !self.close_backup_claimed.swap(true, Ordering::SeqCst)
    }

    pub fn settings(&self) -> AutoSettings {
        self.settings.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Change the rolling settings (keep is clamped), save them and return all settings.
    pub fn set_rolling(&self, enabled: bool, keep: usize) -> std::io::Result<AutoSettings> {
        let mut settings = self
            .settings
            .lock()
            .map_err(|_| std::io::Error::other("settings lock poisoned"))?;
        settings.rolling = Rolling {
            enabled,
            keep: clamp_keep(keep),
        };
        write_json(&self.root.join(SETTINGS_FILE), &*settings)?;
        Ok(settings.clone())
    }

    pub fn rolling_dir(&self) -> PathBuf {
        self.root.join(ROLLING_DIR)
    }

    /// Rolling is on and the last rolling backup is a day old or missing.
    /// Whether the data changed is left to `write_rolling`.
    pub fn rolling_due(&self, now: u64) -> bool {
        if !self.settings().rolling.enabled {
            return false;
        }
        let last = self.state.lock().ok().and_then(|s| s.rolling_at);
        last.is_none_or(|at| now.saturating_sub(at) >= DAY_SECS)
    }

    /// Write a rolling backup unless rolling is off or nothing changed since the last one.
    /// The state is only updated when a file was written.
    pub fn write_rolling_now(&self, user: &UserData) -> Result<Option<PathBuf>> {
        if !self.settings().rolling.enabled {
            return Ok(None);
        }
        self.write_safety_backup(user)
    }

    /// Write a rolling backup before a destructive action (a restore, or
    /// applied sync deletions), even with rolling off: the safety copy
    /// `userData.db.before-restore` does not survive the next merge. Skipped
    /// when nothing changed since the last rolling backup; pruned to `keep`.
    pub fn write_safety_backup(&self, user: &UserData) -> Result<Option<PathBuf>> {
        let rolling = self.settings().rolling;
        let last = self
            .state
            .lock()
            .ok()
            .and_then(|s| s.rolling_fingerprint.clone());
        let Some(written) =
            autobackup::write_rolling(user, &self.rolling_dir(), rolling.keep, last.as_deref())?
        else {
            return Ok(None);
        };
        let state = AutoState {
            rolling_fingerprint: Some(written.fingerprint),
            rolling_at: Some(unix_now()),
        };
        if let Err(e) = write_json(&self.root.join(STATE_FILE), &state) {
            eprintln!("cannot save backup state: {e}");
        }
        if let Ok(mut s) = self.state.lock() {
            *s = state;
        }
        Ok(Some(written.path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pergament::folder_sync::PeerState;
    use pergament::userdata::{Conflict, MergeReport};

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn defaults_when_missing() {
        let dir = tmp();
        let a = Auto::load(dir.path());
        let s = a.settings();
        assert!(s.rolling.enabled);
        assert_eq!(s.rolling.keep, 10);
    }

    #[test]
    fn defaults_when_broken() {
        let dir = tmp();
        fs::write(dir.path().join(SETTINGS_FILE), "{").unwrap();
        assert_eq!(Auto::load(dir.path()).settings(), AutoSettings::default());
    }

    #[test]
    fn partial_file_fills_defaults() {
        let dir = tmp();
        fs::write(dir.path().join(SETTINGS_FILE), r#"{"rolling":{"keep":3}}"#).unwrap();
        let s = Auto::load(dir.path()).settings();
        assert!(s.rolling.enabled);
        assert_eq!(s.rolling.keep, 3);
        assert_eq!(s.sync, SyncSettings::default());
    }

    #[test]
    fn round_trip() {
        let dir = tmp();
        let a = Auto::load(dir.path());
        let saved = a.set_rolling(false, 7).unwrap();
        assert_eq!(Auto::load(dir.path()).settings(), saved);
        assert!(!saved.rolling.enabled);
        assert_eq!(saved.rolling.keep, 7);
    }

    #[test]
    fn rolling_due_cases() {
        let dir = tmp();
        let a = Auto::load(dir.path());
        let now = 10 * DAY_SECS;
        assert!(a.rolling_due(now));
        a.state.lock().unwrap().rolling_at = Some(now - 23 * 3600);
        assert!(!a.rolling_due(now));
        a.state.lock().unwrap().rolling_at = Some(now - 25 * 3600);
        assert!(a.rolling_due(now));
        a.set_rolling(false, 10).unwrap();
        assert!(!a.rolling_due(now));
    }

    #[test]
    fn set_rolling_clamps() {
        assert_eq!(clamp_keep(0), 1);
        assert_eq!(clamp_keep(1000), 100);
        assert_eq!(clamp_keep(10), 10);
        let dir = tmp();
        let a = Auto::load(dir.path());
        assert_eq!(a.set_rolling(true, 0).unwrap().rolling.keep, 1);
        assert_eq!(a.set_rolling(true, 1000).unwrap().rolling.keep, 100);
    }

    #[test]
    fn close_backup_is_claimed_once() {
        let dir = tmp();
        let a = Auto::load(dir.path());
        assert!(a.claim_close_backup());
        assert!(!a.claim_close_backup());
        assert!(Auto::load(dir.path()).claim_close_backup());
    }

    #[test]
    fn write_json_leaves_no_temp_file() {
        let dir = tmp();
        Auto::load(dir.path()).set_rolling(true, 5).unwrap();
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, vec![SETTINGS_FILE.to_owned()]);
    }

    #[test]
    fn write_rolling_now_records_state_only_when_written() {
        use pergament::userdata::DB_NAME;
        let dir = tmp();
        let ud = UserData::open(dir.path().join(DB_NAME)).unwrap();
        let a = Auto::load(dir.path());

        let path = a.write_rolling_now(&ud).unwrap().expect("first write");
        assert!(path.exists());
        let first = a.state.lock().unwrap().clone();
        assert!(first.rolling_fingerprint.is_some());
        assert!(first.rolling_at.is_some());

        // Unchanged data: nothing written, state untouched.
        let before = a.state.lock().unwrap().clone();
        assert!(a.write_rolling_now(&ud).unwrap().is_none());
        assert_eq!(*a.state.lock().unwrap(), before);

        // The state survives a restart.
        assert_eq!(*Auto::load(dir.path()).state.lock().unwrap(), first);

        // Rolling off: nothing written, state untouched.
        a.set_rolling(false, 10).unwrap();
        assert!(a.write_rolling_now(&ud).unwrap().is_none());
        assert_eq!(*a.state.lock().unwrap(), first);
    }

    #[test]
    fn safety_backup_ignores_the_switch_but_not_the_fingerprint() {
        use pergament::userdata::{BLOCK_VERSE, DB_NAME, Loc, Range};
        let dir = tmp();
        let mut ud = UserData::open(dir.path().join(DB_NAME)).unwrap();
        let a = Auto::load(dir.path());
        a.set_rolling(false, 1).unwrap();

        let first = a
            .write_safety_backup(&ud)
            .unwrap()
            .expect("written while off");
        assert!(first.exists());
        let state = a.state.lock().unwrap().clone();
        assert!(state.rolling_fingerprint.is_some());
        // Unchanged data: no second copy.
        assert!(a.write_safety_backup(&ud).unwrap().is_none());

        let loc = Loc {
            key_symbol: "nwtsty".into(),
            meps_language: 2,
            issue_tag: 0,
            document_id: None,
            book: Some(19),
            chapter: Some(23),
        };
        let range = Range {
            block_type: BLOCK_VERSE,
            identifier: 1,
            start: 0,
            end: 3,
        };
        ud.add_mark(&loc, None, 3, &[range]).unwrap();
        let second = a.write_safety_backup(&ud).unwrap().expect("data changed");
        assert_ne!(*a.state.lock().unwrap(), state);
        // Still pruned to `keep`.
        let names: Vec<_> = fs::read_dir(a.rolling_dir())
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(names, vec![second]);
    }

    #[test]
    fn safety_backup_only_for_applied_deletions() {
        let file = "Pergament-a-000001.jwlibrary".to_owned();
        assert!(!needs_safety_backup(&BTreeMap::new()));
        assert!(!needs_safety_backup(&BTreeMap::from([(
            file.clone(),
            DeleteGuard::Skip
        )])));
        assert!(needs_safety_backup(&BTreeMap::from([(
            file,
            DeleteGuard::Apply
        )])));
    }

    #[test]
    fn debounce_coalesces() {
        let t0 = Instant::now();
        let mut s = Scheduler::default();
        s.changed(t0);
        s.changed(t0 + Duration::from_secs(2));
        s.changed(t0 + Duration::from_secs(4));
        // 5 s after the last change, not the first.
        assert!(!s.take_due(t0 + Duration::from_secs(8)).write);
        assert_eq!(s.next_deadline(), Some(t0 + Duration::from_secs(9)));
        assert!(s.take_due(t0 + Duration::from_secs(9)).write);
        assert!(!s.take_due(t0 + Duration::from_secs(20)).write);
        assert_eq!(s.next_deadline(), None);
    }

    #[test]
    fn folder_events_are_debounced_and_polled() {
        let t0 = Instant::now();
        let mut s = Scheduler::default();
        s.folder_changed(t0);
        s.folder_changed(t0 + Duration::from_secs(1));
        assert!(!s.take_due(t0 + Duration::from_secs(2)).sync);
        assert!(s.take_due(t0 + Duration::from_secs(3)).sync);
        s.synced(t0 + Duration::from_secs(3));
        assert_eq!(
            s.next_deadline(),
            Some(t0 + Duration::from_secs(3) + POLL_INTERVAL)
        );
        assert!(s.take_due(t0 + Duration::from_secs(3) + POLL_INTERVAL).sync);
    }

    #[test]
    fn watcher_is_renewed_when_the_folder_returns() {
        let mut h = FolderHealth::default();
        assert!(!h.round(true));
        assert!(!h.round(false));
        assert!(!h.round(false));
        assert!(h.round(true), "back after an error");
        assert!(!h.round(true), "only once");
    }

    #[test]
    fn close_flushes_pending_write() {
        let t0 = Instant::now();
        let mut s = Scheduler::default();
        assert!(!s.on_close(), "nothing changed, nothing to write");
        let mut s = Scheduler::default();
        s.changed(t0);
        assert!(s.on_close());
        assert!(s.is_closed());
        // The worker writes nothing after the close.
        assert!(!s.take_due(t0 + Duration::from_secs(10)).write);

        // A failed write is retried on close.
        let mut s = Scheduler::default();
        s.changed(t0);
        s.begin_write();
        assert!(!s.on_close());
        let mut s = Scheduler::default();
        s.changed(t0);
        s.begin_write();
        s.write_failed();
        assert!(s.on_close());
    }

    fn conflict(n: usize) -> Conflict {
        Conflict {
            table: "UserMark".into(),
            guid: format!("g{n}"),
            summary: String::new(),
        }
    }

    #[test]
    fn status_collects_pending_and_conflicts() {
        let mut status = SyncStatus::default();
        let report = MergeReport {
            added: 1,
            conflicts: (0..40).map(conflict).collect(),
            ..MergeReport::default()
        };
        let changed = status.record(
            vec![
                PeerResult::Merged {
                    file: "Pergament-a-000001.jwlibrary".into(),
                    device: "Pergament (a)".into(),
                    report,
                    deleted: 0,
                },
                PeerResult::NeedsConfirmation {
                    file: "Pergament-b-000002.jwlibrary".into(),
                    device: "Pergament (b)".into(),
                    deletions: 9,
                },
                PeerResult::Failed {
                    file: "Pergament-c-000003.jwlibrary".into(),
                    error: "broken".into(),
                },
            ],
            100,
        );
        assert!(changed);
        assert_eq!(status.last_sync, Some(100));
        assert_eq!(status.last_device.as_deref(), Some("Pergament (a)"));
        assert_eq!(
            status.pending,
            vec![PendingPeer {
                file: "Pergament-b-000002.jwlibrary".into(),
                device: "Pergament (b)".into(),
                deletions: 9,
            }]
        );
        assert!(status.error.as_deref().unwrap().contains("broken"));
        assert_eq!(status.conflicts.len(), 40);

        // An unchanged merge changes nothing; the newest 50 conflicts are kept;
        // a resolved peer leaves the pending list.
        let report = MergeReport {
            conflicts: (40..60).map(conflict).collect(),
            ..MergeReport::default()
        };
        let changed = status.record(
            vec![PeerResult::Merged {
                file: "Pergament-b-000002.jwlibrary".into(),
                device: "Pergament (b)".into(),
                report,
                deleted: 0,
            }],
            200,
        );
        assert!(!changed);
        assert!(status.pending.is_empty());
        assert!(status.error.is_none());
        assert_eq!(status.conflicts.len(), MAX_CONFLICTS);
        assert_eq!(status.conflicts[0].guid, "g10");
        assert_eq!(status.conflicts[49].guid, "g59");

        let value = serde_json::to_value(&status).unwrap();
        for key in [
            "folder",
            "lastSync",
            "lastDevice",
            "error",
            "pending",
            "conflicts",
        ] {
            assert!(value.get(key).is_some(), "missing {key}");
        }
    }

    #[test]
    fn add_error_keeps_the_round_errors() {
        let mut status = SyncStatus::default();
        status.add_error("backup failed".into());
        assert_eq!(status.error.as_deref(), Some("backup failed"));
        status.record(
            vec![PeerResult::Failed {
                file: "f".into(),
                error: "broken".into(),
            }],
            1,
        );
        status.add_error("backup failed".into());
        assert_eq!(status.error.as_deref(), Some("f: broken\nbackup failed"));
    }

    #[test]
    fn pending_follows_the_sync_state() {
        let mut status = SyncStatus::default();
        status.pending.push(PendingPeer {
            file: "Pergament-gone-000009.jwlibrary".into(),
            device: "Pergament (gone)".into(),
            deletions: 3,
        });
        let mut state = SyncState::default();
        state.peers.insert(
            "Pergament-my-box-00abcd.jwlibrary".into(),
            PeerState {
                merged_hash: "h".into(),
                pending: Some(7),
            },
        );
        status.sync_pending(&state);
        assert_eq!(
            status.pending,
            vec![PendingPeer {
                file: "Pergament-my-box-00abcd.jwlibrary".into(),
                device: "Pergament (my-box)".into(),
                deletions: 7,
            }]
        );
    }

    #[test]
    fn set_sync_folder_refuses_the_library_and_its_backups() {
        let dir = tmp();
        let a = Auto::load(dir.path());
        fs::create_dir_all(a.rolling_dir()).unwrap();
        for refused in [
            dir.path().to_owned(),
            a.rolling_dir(),
            dir.path().join(ROLLING_DIR).join("."),
        ] {
            let err = a.set_sync_folder(Some(refused.clone())).unwrap_err();
            assert_eq!(err.to_string(), SYNC_IN_LIBRARY, "{}", refused.display());
            assert_eq!(a.settings().sync, SyncSettings::default());
        }
        assert!(!dir.path().join(SETTINGS_FILE).exists());
        // A folder elsewhere is fine.
        let shared = tmp();
        assert!(a.set_sync_folder(Some(shared.path().to_owned())).is_ok());
    }

    #[test]
    fn device_name_is_stored_with_the_id() {
        let dir = tmp();
        let shared = tmp();
        let mut a = Auto::load(dir.path());
        a.host = "My Mac.local".into();
        let set = a.set_sync_folder(Some(shared.path().to_owned())).unwrap();
        assert_eq!(set.sync.host.as_deref(), Some("My-Mac-local"));
        let own = a.sync_folder().unwrap().own;
        let id = set.sync.device_id.unwrap();
        assert_eq!(own, format!("Pergament-My-Mac-local-{id}.jwlibrary"));
        // Round trip, and a renamed computer keeps its file name.
        let mut b = Auto::load(dir.path());
        assert_eq!(b.settings(), a.settings());
        b.host = "renamed".into();
        assert_eq!(b.sync_folder().unwrap().own, own);
    }

    #[test]
    fn old_settings_get_the_device_name_on_first_use() {
        let dir = tmp();
        let shared = tmp();
        let old = serde_json::json!({ "sync": { "folder": shared.path(), "deviceId": "00abcd" } });
        fs::write(dir.path().join(SETTINGS_FILE), old.to_string()).unwrap();
        let mut a = Auto::load(dir.path());
        assert_eq!(a.settings().sync.host, None);
        a.host = "box one".into();
        assert_eq!(
            a.sync_folder().unwrap().own,
            "Pergament-box-one-00abcd.jwlibrary"
        );
        let mut b = Auto::load(dir.path());
        assert_eq!(b.settings().sync.host.as_deref(), Some("box-one"));
        b.host = "renamed".into();
        assert_eq!(
            b.sync_folder().unwrap().own,
            "Pergament-box-one-00abcd.jwlibrary"
        );
    }

    #[test]
    fn set_sync_folder_none_forgets() {
        let dir = tmp();
        let shared = tmp();
        let a = Auto::load(dir.path());
        let set = a.set_sync_folder(Some(shared.path().to_owned())).unwrap();
        assert_eq!(set.sync.folder.as_deref(), Some(shared.path()));
        let id = set.sync.device_id.clone().expect("device id created");
        let folder = a.sync_folder().expect("sync on");
        assert!(folder.own.ends_with(&format!("-{id}.jwlibrary")));

        // Some state and a base, as a sync leaves them.
        SyncState::default().save(&folder.state_dir).unwrap();
        fs::create_dir_all(folder.state_dir.join("base")).unwrap();
        fs::write(folder.state_dir.join("base/x.json"), "{}").unwrap();
        fs::write(shared.path().join(&folder.own), "mine").unwrap();

        let off = a.set_sync_folder(None).unwrap();
        assert_eq!(off.sync.folder, None);
        assert_eq!(off.sync.device_id.as_deref(), Some(id.as_str()), "id kept");
        assert!(a.sync_folder().is_none());
        assert!(!folder.state_dir.join("state.json").exists());
        assert!(!folder.state_dir.join("base").exists());
        assert!(
            shared.path().join(&folder.own).exists(),
            "folder left alone"
        );
        assert_eq!(Auto::load(dir.path()).settings(), off);
    }
}
