//! Folder sync: every computer writes only its own `Pergament-<host>-<id>.jwlibrary`
//! into a folder that something else (Nextcloud, Syncthing, ...) keeps in
//! sync, and merges the other computers' files three-way (docs/FORMAT.md,
//! "Sync"). Everything in the folder is a valid `.jwlibrary` backup; the
//! sync state lives outside it.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::Result;
use crate::autobackup;
use crate::sync_snapshot::{DeleteGuard, Snapshot, SyncOutcome};
use crate::userdata::{MergeReport, UserData, backup_identity};

const STATE_FILE: &str = "state.json";
const BASE_DIR: &str = "base";

/// Write `bytes` to `path` through a temp file in the same directory.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".write-")
        .tempfile_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Flush a directory's entries (a rename into it), where the system
/// allows; best effort.
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(dir) = fs::File::open(dir) {
        let _ = dir.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

/// What is known about one peer file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerState {
    /// The manifest hash of the file as last seen: merged, or held back for
    /// a choice (then `pending` is set).
    pub merged_hash: String,
    /// Deletions of that file awaiting the user's choice.
    #[serde(default)]
    pub pending: Option<i64>,
}

/// The sync state of this computer, kept in `<state_dir>/state.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncState {
    /// The database this state was written for ([`UserData::generation`]).
    #[serde(default)]
    pub database: Option<i64>,
    /// Fingerprint of the data in this computer's own file.
    #[serde(default)]
    pub last_written: Option<String>,
    /// By file name.
    #[serde(default)]
    pub peers: BTreeMap<String, PeerState>,
}

impl SyncState {
    /// The state in `state_dir`; empty when there is none or it is unreadable
    /// (a merge without a base only adds, so that is safe).
    pub fn load(state_dir: &Path) -> Self {
        fs::read(state_dir.join(STATE_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Write the state to `state_dir`, atomically.
    pub fn save(&self, state_dir: &Path) -> Result<()> {
        write_atomic(
            &state_dir.join(STATE_FILE),
            &serde_json::to_vec_pretty(self)?,
        )
    }
}

/// The result of syncing one peer file.
#[derive(Debug)]
pub enum PeerResult {
    Merged {
        file: String,
        device: String,
        report: MergeReport,
        deleted: i64,
    },
    NeedsConfirmation {
        file: String,
        device: String,
        deletions: i64,
    },
    Failed {
        file: String,
        error: String,
    },
}

/// A sync folder as seen from this computer.
#[derive(Debug, Clone)]
pub struct SyncFolder {
    /// The shared folder.
    pub dir: PathBuf,
    /// File name of this computer's own file.
    pub own: String,
    /// Where state.json and `base/` live (not in the shared folder).
    pub state_dir: PathBuf,
}

/// `Pergament-<host>-<id>.jwlibrary`, the host reduced to `[A-Za-z0-9-]`.
pub fn device_file_name(host: &str, id: &str) -> String {
    format!("Pergament-{}-{id}.jwlibrary", file_host(host))
}

/// `host` as it goes into a file name: reduced to `[A-Za-z0-9-]`, and
/// `computer` when empty.
pub fn file_host(host: &str) -> String {
    let host: String = host
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    if host.is_empty() {
        "computer".into()
    } else {
        host
    }
}

/// Six lowercase hex characters.
pub fn new_device_id() -> String {
    let mut bytes = [0u8; 3];
    let from_system = fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .is_ok();
    if !from_system {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let digest = Sha256::digest(format!("{now}-{}", std::process::id()));
        bytes.copy_from_slice(&digest[..3]);
    }
    hex::encode(bytes)
}

/// This computer's name.
pub fn hostname() -> String {
    gethostname::gethostname().to_string_lossy().into_owned()
}

impl SyncFolder {
    /// The other computers' files in the folder. An error when the folder
    /// cannot be read.
    pub fn peers(&self) -> Result<Vec<PathBuf>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name != self.own && is_device_file(&name) && entry.path().is_file() {
                out.push(entry.path());
            }
        }
        out.sort();
        Ok(out)
    }

    /// Write this computer's file unless it holds the current data already
    /// or there are no synced rows. Returns whether it was written.
    pub fn write_own(&self, ud: &UserData, host: &str, state: &mut SyncState) -> Result<bool> {
        // An empty file is published only when this very database held
        // synced rows and the user deleted them. After a reset, a new file
        // or a "Stop syncing", the state is not this database's, so an
        // empty file would delete the rows other computers still hold.
        if ud.snapshot()?.0.is_empty() {
            let deliberate = state.database == Some(ud.generation()?) && self.has_bases(state);
            if !deliberate {
                return Ok(false);
            }
        }
        let dest = self.dir.join(&self.own);
        let fingerprint = ud.fingerprint()?;
        if state.last_written.as_deref() == Some(fingerprint.as_str()) && dest.is_file() {
            return Ok(false);
        }
        let part = self.dir.join(format!(".{}.part", self.own));
        // The export flushes the file before the rename.
        let written = ud
            .export_backup_as(&part, &format!("Pergament ({host})"))
            .and_then(|()| Ok(fs::rename(&part, &dest)?));
        if let Err(e) = written {
            let _ = fs::remove_file(&part);
            return Err(e);
        }
        sync_dir(&self.dir);
        state.last_written = Some(fingerprint);
        state.save(&self.state_dir)?;
        Ok(true)
    }

    /// Merge every peer file that changed, or that is held back and has a
    /// choice for the file as it still is. An error when the folder cannot
    /// be read.
    pub fn sync_peers(
        &self,
        ud: &mut UserData,
        state: &mut SyncState,
        choices: &BTreeMap<String, DeleteGuard>,
    ) -> Result<Vec<PeerResult>> {
        let mut results = Vec::new();
        let files = self.peers()?;
        self.reconcile_database(ud, state)?;
        for path in files {
            let file = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if let Some(result) = self.sync_peer(ud, state, choices, &path, &file) {
                results.push(result);
            }
        }
        Ok(results)
    }

    /// A private copy of the peer file, made once per round.
    fn copy_peer(&self, path: &Path) -> Result<tempfile::NamedTempFile> {
        fs::create_dir_all(&self.state_dir)?;
        let mut copy = tempfile::Builder::new()
            .prefix(".peer-")
            .tempfile_in(&self.state_dir)?;
        std::io::copy(&mut fs::File::open(path)?, copy.as_file_mut())?;
        copy.as_file_mut().flush()?;
        Ok(copy)
    }

    /// `None` when the file needs nothing.
    fn sync_peer(
        &self,
        ud: &mut UserData,
        state: &mut SyncState,
        choices: &BTreeMap<String, DeleteGuard>,
        path: &Path,
        file: &str,
    ) -> Option<PeerResult> {
        let failed = |error: crate::Error| PeerResult::Failed {
            file: file.to_owned(),
            error: error.to_string(),
        };
        // The identity and the merge read the same copy: a peer file that
        // is replaced in between cannot be recorded under the old hash.
        let copy = match self.copy_peer(path) {
            Ok(copy) => copy,
            Err(e) => return Some(failed(e)),
        };
        let (device, hash) = match backup_identity(copy.path()) {
            Ok(identity) => identity,
            Err(e) => return Some(failed(e)),
        };
        let known = state.peers.get(file);
        // A choice answers the question the user saw: the peer is pending
        // and its file is the one held back. Anything else is checked anew.
        let choice = choices
            .get(file)
            .copied()
            .filter(|_| known.is_some_and(|p| p.pending.is_some() && p.merged_hash == hash));
        if choice.is_none() && known.is_some_and(|p| p.merged_hash == hash) {
            return None;
        }
        let base = self.load_base(file);
        let guard = choice.unwrap_or(DeleteGuard::Check);
        match ud.sync_merge(copy.path(), base.as_ref(), guard) {
            Err(e) => Some(failed(e)),
            Ok(SyncOutcome::NeedsConfirmation { deletions, .. }) => {
                state.peers.insert(
                    file.to_owned(),
                    PeerState {
                        merged_hash: hash,
                        pending: Some(deletions),
                    },
                );
                let _ = state.save(&self.state_dir);
                Some(PeerResult::NeedsConfirmation {
                    file: file.to_owned(),
                    device,
                    deletions,
                })
            }
            Ok(SyncOutcome::Merged(merge)) => {
                // The data is merged already; without a stored base the next
                // round merges again, which only adds.
                if let Err(e) = self.store_base(file, &merge.snapshot) {
                    return Some(failed(e));
                }
                state.peers.insert(
                    file.to_owned(),
                    PeerState {
                        merged_hash: hash,
                        pending: None,
                    },
                );
                if let Err(e) = state.save(&self.state_dir) {
                    return Some(failed(e));
                }
                Some(PeerResult::Merged {
                    file: file.to_owned(),
                    device,
                    report: merge.report,
                    deleted: merge.deleted,
                })
            }
        }
    }

    /// The synced data here are gone (no highlight, note, user tag,
    /// bookmark or answer) while bases from earlier merges hold rows: the
    /// database was lost or reset, and the other computers can refill it.
    /// Empty bases, as between two empty computers, do not count.
    fn is_emptied(&self, ud: &UserData, state: &SyncState) -> Result<bool> {
        if !self.has_bases(state) || !ud.snapshot()?.0.is_empty() {
            return Ok(false);
        }
        let Ok(entries) = fs::read_dir(self.state_dir.join(BASE_DIR)) else {
            return Ok(false);
        };
        Ok(entries.flatten().any(|entry| {
            fs::read(entry.path())
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Snapshot>(&bytes).ok())
                .is_some_and(|base| !base.0.is_empty())
        }))
    }

    /// Merged a peer before: a peer state or a stored base.
    fn has_bases(&self, state: &SyncState) -> bool {
        !state.peers.is_empty()
            || fs::read_dir(self.state_dir.join(BASE_DIR))
                .is_ok_and(|mut entries| entries.next().is_some())
    }

    /// Bind the state to this database. The bases were made for another
    /// database (reset, replaced or restored from elsewhere), so they are
    /// dropped: the merges that follow only add. A state from before the
    /// database id was stored is dropped only when its database is emptied.
    fn reconcile_database(&self, ud: &UserData, state: &mut SyncState) -> Result<()> {
        let generation = ud.generation()?;
        let other_database = match state.database {
            Some(known) => known != generation,
            None => self.is_emptied(ud, state)?,
        };
        if other_database {
            self.forget();
            *state = SyncState::default();
        }
        if state.database != Some(generation) {
            state.database = Some(generation);
            state.save(&self.state_dir)?;
        }
        Ok(())
    }

    fn base_path(&self, file: &str) -> PathBuf {
        self.state_dir.join(BASE_DIR).join(format!("{file}.json"))
    }

    fn load_base(&self, file: &str) -> Option<Snapshot> {
        let bytes = fs::read(self.base_path(file)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn store_base(&self, file: &str, snapshot: &Snapshot) -> Result<()> {
        write_atomic(&self.base_path(file), &serde_json::to_vec(snapshot)?)
    }

    /// "Stop syncing": drop the state and the bases, leave the folder alone.
    pub fn forget(&self) {
        let _ = fs::remove_dir_all(self.state_dir.join(BASE_DIR));
        let _ = fs::remove_file(self.state_dir.join(STATE_FILE));
    }
}

/// `Pergament-<host>-<6 lowercase hex>.jwlibrary` with a host of `[A-Za-z0-9-]`,
/// and not a rolling backup's name.
pub fn is_device_file(name: &str) -> bool {
    let Some(stem) = name
        .strip_prefix("Pergament-")
        .and_then(|n| n.strip_suffix(".jwlibrary"))
    else {
        return false;
    };
    let Some((host, id)) = stem.rsplit_once('-') else {
        return false;
    };
    // A rolling backup (`Pergament-YYYY-MM-DD-HHMMSS`) has the same shape.
    !autobackup::is_rolling_name(name)
        && !host.is_empty()
        && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && id.len() == 6
        && id.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::userdata::{BLOCK_VERSE, Loc, Range};
    use std::fs;

    fn bible(chapter: i64) -> Loc {
        Loc {
            key_symbol: "nwtsty".into(),
            meps_language: 2,
            issue_tag: 0,
            document_id: None,
            book: Some(19),
            chapter: Some(chapter),
        }
    }

    fn verse(v: i64) -> Range {
        Range {
            block_type: BLOCK_VERSE,
            identifier: v,
            start: 0,
            end: 3,
        }
    }

    struct Dev {
        ud: UserData,
        folder: SyncFolder,
        state: SyncState,
        host: &'static str,
    }

    fn dev(tmp: &Path, name: &'static str, id: &str) -> Dev {
        let root = tmp.join(name);
        let own = device_file_name(name, id);
        Dev {
            ud: UserData::open(root.join("userData.db")).unwrap(),
            folder: SyncFolder {
                dir: tmp.join("shared"),
                own,
                state_dir: root.join("sync"),
            },
            state: SyncState::default(),
            host: name,
        }
    }

    impl Dev {
        fn write(&mut self) -> bool {
            self.folder
                .write_own(&self.ud, self.host, &mut self.state)
                .unwrap()
        }
        fn sync(&mut self) -> Vec<PeerResult> {
            self.sync_with(&BTreeMap::new())
        }
        fn sync_with(&mut self, choices: &BTreeMap<String, DeleteGuard>) -> Vec<PeerResult> {
            self.folder
                .sync_peers(&mut self.ud, &mut self.state, choices)
                .unwrap()
        }
        fn mark(&mut self, chapter: i64, v: i64) -> String {
            self.ud
                .add_mark(&bible(chapter), None, 3, &[verse(v)])
                .unwrap()
        }
    }

    fn setup() -> (tempfile::TempDir, Dev, Dev) {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("shared")).unwrap();
        let a = dev(tmp.path(), "alpha", "aaaaaa");
        let b = dev(tmp.path(), "beta", "bbbbbb");
        (tmp, a, b)
    }

    fn merged(results: &[PeerResult]) -> usize {
        results
            .iter()
            .filter(|r| matches!(r, PeerResult::Merged { .. }))
            .count()
    }

    #[test]
    fn file_name_sanitizes_host() {
        assert_eq!(
            device_file_name("my host_1", "abc123"),
            "Pergament-my-host-1-abc123.jwlibrary"
        );
        assert_eq!(
            device_file_name("", "abc123"),
            "Pergament-computer-abc123.jwlibrary"
        );
        let id = new_device_id();
        assert_eq!(id.len(), 6);
        assert!(id.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')));
        assert!(!hostname().is_empty());
    }

    #[test]
    fn round_trip_two_devices() {
        let (_tmp, mut a, mut b) = setup();
        a.mark(1, 1);
        a.mark(1, 2);
        assert!(a.write());
        let r = b.sync();
        assert_eq!(merged(&r), 1);
        match &r[0] {
            PeerResult::Merged { device, file, .. } => {
                assert_eq!(device, "Pergament (alpha)");
                assert_eq!(file, &a.folder.own);
            }
            other => panic!("{other:?}"),
        }
        b.mark(2, 5);
        assert!(b.write());
        assert_eq!(merged(&a.sync()), 1);
        assert_eq!(a.ud.snapshot().unwrap(), b.ud.snapshot().unwrap());
        assert_eq!(a.ud.snapshot().unwrap().0.len(), 3);
        // Reloading the state from disk keeps what was stored.
        let loaded = SyncState::load(&a.folder.state_dir);
        assert_eq!(loaded, a.state);
    }

    #[test]
    fn write_own_skips_unchanged() {
        let (_tmp, mut a, _b) = setup();
        a.mark(1, 1);
        assert!(a.write());
        assert!(!a.write());
        a.mark(1, 2);
        assert!(a.write());
        // A file removed from the folder is written again.
        fs::remove_file(a.folder.dir.join(&a.folder.own)).unwrap();
        assert!(a.write());
        assert!(a.folder.dir.join(&a.folder.own).exists());
        assert!(
            !a.folder
                .dir
                .join(format!(".{}.part", a.folder.own))
                .exists()
        );
    }

    #[test]
    fn ignores_foreign_files() {
        let (_tmp, mut a, mut b) = setup();
        a.mark(1, 1);
        a.write();
        b.write();
        let dir = &a.folder.dir;
        for name in [
            "Pergament-a-abc123 (conflicted copy).jwlibrary",
            ".Pergament-a-abc123.jwlibrary.part",
            "UserdataBackup_2026-10-20_Phone.jwlibrary",
            "Pergament-a-ABC123.jwlibrary",
            "Pergament--abc123.jwlibrary",
            "Pergament-a-abc12.jwlibrary",
            // Rolling backups, should someone pick the backups folder.
            "Pergament-2026-10-20-142501.jwlibrary",
            "notes.txt",
        ] {
            fs::write(dir.join(name), b"x").unwrap();
        }
        fs::create_dir(dir.join("Pergament-d-abc123.jwlibrary")).unwrap();
        let peers = b.folder.peers().unwrap();
        assert_eq!(peers, vec![dir.join(&a.folder.own)]);
    }

    #[test]
    fn broken_peer_skipped_and_retried() {
        let (_tmp, mut a, mut b) = setup();
        a.mark(1, 1);
        a.write();
        let path = a.folder.dir.join(&a.folder.own);
        let good = fs::read(&path).unwrap();
        fs::write(&path, &good[..good.len() / 2]).unwrap();
        let before = b.state.clone();
        let local = b.ud.snapshot().unwrap();
        let r = b.sync();
        assert!(matches!(&r[0], PeerResult::Failed { file, .. } if *file == a.folder.own));
        assert_eq!(b.state.peers, before.peers);
        assert_eq!(b.ud.snapshot().unwrap(), local);
        fs::write(&path, &good).unwrap();
        assert_eq!(merged(&b.sync()), 1);
        assert_eq!(b.ud.snapshot().unwrap(), a.ud.snapshot().unwrap());
    }

    #[test]
    fn pending_mass_delete_then_choice() {
        let (_tmp, mut a, mut b) = setup();
        let guids: Vec<String> = (1..=12).map(|i| a.mark(1, i)).collect();
        a.write();
        assert_eq!(merged(&b.sync()), 1);
        for g in &guids[..10] {
            a.ud.delete_mark(g).unwrap();
        }
        a.write();
        let file = a.folder.own.clone();
        let r = b.sync();
        assert!(
            matches!(&r[0], PeerResult::NeedsConfirmation { deletions, .. } if *deletions == 10),
            "{r:?}"
        );
        assert_eq!(b.state.peers[&file].pending, Some(10));
        assert_eq!(b.ud.snapshot().unwrap().0.len(), 12);
        // Without a choice and without a change, it stays held back quietly.
        assert!(b.sync().is_empty());
        assert_eq!(b.state.peers[&file].pending, Some(10));
        let choices = BTreeMap::from([(file.clone(), DeleteGuard::Skip)]);
        let r = b.sync_with(&choices);
        assert_eq!(merged(&r), 1, "{r:?}");
        assert_eq!(b.state.peers[&file].pending, None);
        assert_eq!(b.ud.snapshot().unwrap().0.len(), 12);
        // Handled: nothing more to do.
        assert!(b.sync().is_empty());
    }

    #[test]
    fn repeated_sync_without_changes_does_nothing() {
        let (_tmp, mut a, mut b) = setup();
        a.mark(1, 1);
        a.write();
        assert_eq!(merged(&b.sync()), 1);
        let fp = b.ud.fingerprint().unwrap();
        let r = b.sync();
        assert!(r.is_empty(), "{r:?}");
        assert_eq!(b.ud.fingerprint().unwrap(), fp);
        // Own write after merging, then another round: still quiet.
        b.write();
        assert!(b.sync().is_empty());
        assert!(
            a.sync()
                .iter()
                .all(|r| !matches!(r, PeerResult::Failed { .. }))
        );
        assert!(a.sync().is_empty());
    }

    #[test]
    fn missing_folder_is_an_error_not_a_change() {
        let (_tmp, mut a, mut b) = setup();
        a.mark(1, 1);
        a.write();
        b.sync();
        b.mark(3, 3);
        let snapshot = b.ud.snapshot().unwrap();
        let state = b.state.clone();
        fs::remove_dir_all(&b.folder.dir).unwrap();
        assert!(b.folder.peers().is_err());
        assert!(
            b.folder
                .sync_peers(&mut b.ud, &mut b.state, &BTreeMap::new())
                .is_err()
        );
        assert_eq!(b.ud.snapshot().unwrap(), snapshot);
        assert_eq!(b.state, state);
    }

    #[test]
    fn forget_clears_state_only() {
        let (_tmp, mut a, mut b) = setup();
        a.mark(1, 1);
        a.write();
        b.sync();
        b.write();
        let state_dir = &b.folder.state_dir;
        assert!(state_dir.join("state.json").exists());
        assert!(
            state_dir
                .join("base")
                .join(format!("{}.json", a.folder.own))
                .exists()
        );
        let data = b.ud.snapshot().unwrap();
        b.folder.forget();
        assert!(!state_dir.join("state.json").exists());
        assert!(!state_dir.join("base").exists());
        assert_eq!(fs::read_dir(&b.folder.dir).unwrap().count(), 2);
        assert_eq!(b.ud.snapshot().unwrap(), data);
        assert_eq!(SyncState::load(state_dir), SyncState::default());
    }

    /// A has 12 marks that B holds too; A then deletes `n` of them.
    fn mass_delete_setup(n: usize) -> (tempfile::TempDir, Dev, Dev, Vec<String>) {
        let (tmp, mut a, mut b) = setup();
        let guids: Vec<String> = (1..=12).map(|i| a.mark(1, i)).collect();
        a.write();
        assert_eq!(merged(&b.sync()), 1);
        for g in &guids[..n] {
            a.ud.delete_mark(g).unwrap();
        }
        a.write();
        (tmp, a, b, guids)
    }

    #[test]
    fn pending_rechecked_when_the_file_changes() {
        let (_tmp, mut a, mut b, guids) = mass_delete_setup(10);
        let file = a.folder.own.clone();
        assert!(matches!(
            &b.sync()[0],
            PeerResult::NeedsConfirmation { deletions: 10, .. }
        ));
        // The peer deletes one more: a new file, checked again.
        a.ud.delete_mark(&guids[10]).unwrap();
        a.write();
        let r = b.sync();
        assert!(
            matches!(&r[0], PeerResult::NeedsConfirmation { deletions: 11, .. }),
            "{r:?}"
        );
        assert_eq!(b.state.peers[&file].pending, Some(11));
        assert_eq!(b.ud.snapshot().unwrap().0.len(), 12);
    }

    #[test]
    fn choice_for_a_file_that_changed_is_ignored() {
        let (_tmp, mut a, mut b, guids) = mass_delete_setup(10);
        let file = a.folder.own.clone();
        assert!(matches!(
            &b.sync()[0],
            PeerResult::NeedsConfirmation { deletions: 10, .. }
        ));
        a.ud.delete_mark(&guids[10]).unwrap();
        a.write();
        let choices = BTreeMap::from([(file.clone(), DeleteGuard::Apply)]);
        let r = b.sync_with(&choices);
        assert!(
            matches!(&r[0], PeerResult::NeedsConfirmation { deletions: 11, .. }),
            "{r:?}"
        );
        assert_eq!(b.ud.snapshot().unwrap().0.len(), 12);
        assert_eq!(b.state.peers[&file].pending, Some(11));
    }

    #[test]
    fn apply_deletes_and_clears_pending() {
        let (_tmp, a, mut b, _guids) = mass_delete_setup(10);
        let file = a.folder.own.clone();
        assert!(matches!(&b.sync()[0], PeerResult::NeedsConfirmation { .. }));
        let choices = BTreeMap::from([(file.clone(), DeleteGuard::Apply)]);
        let r = b.sync_with(&choices);
        assert!(
            matches!(&r[0], PeerResult::Merged { deleted: 10, .. }),
            "{r:?}"
        );
        assert_eq!(b.state.peers[&file].pending, None);
        assert_eq!(b.ud.snapshot().unwrap().0.len(), 2);
        assert_eq!(b.ud.snapshot().unwrap(), a.ud.snapshot().unwrap());
    }

    fn conflicts(results: &[PeerResult]) -> usize {
        results
            .iter()
            .map(|r| match r {
                PeerResult::Merged { report, .. } => report.conflicts.len(),
                _ => 0,
            })
            .sum()
    }

    #[test]
    fn concurrent_edits_converge() {
        let (_tmp, mut a, mut b) = setup();
        let m = a.mark(1, 1);
        a.write();
        assert_eq!(merged(&b.sync()), 1);
        b.write();
        a.sync();
        // Both recolor the same highlight differently and add another one.
        a.ud.set_mark_color(&m, 1).unwrap();
        b.ud.set_mark_color(&m, 2).unwrap();
        a.mark(2, 1);
        b.mark(3, 1);
        a.write();
        b.write();
        let mut reported = conflicts(&a.sync()) + conflicts(&b.sync());
        for _ in 0..2 {
            a.write();
            b.write();
            reported += conflicts(&a.sync()) + conflicts(&b.sync());
        }
        assert!(reported > 0, "the conflict is listed");
        assert_eq!(a.ud.snapshot().unwrap(), b.ud.snapshot().unwrap());
        assert_eq!(a.ud.snapshot().unwrap().0.len(), 3);
    }

    /// A and B synced `n` marks; then B's database is lost while its sync
    /// state survives.
    fn reset_setup(n: i64) -> (tempfile::TempDir, Dev, Dev) {
        let (tmp, mut a, mut b) = setup();
        for i in 1..=n {
            a.mark(1, i);
        }
        a.write();
        assert_eq!(merged(&b.sync()), 1);
        b.mark(2, 1);
        b.write();
        assert_eq!(merged(&a.sync()), 1);
        a.write();
        b.sync();
        assert_eq!(a.ud.snapshot().unwrap(), b.ud.snapshot().unwrap());
        let path = b.ud.path().to_owned();
        drop(std::mem::replace(
            &mut b.ud,
            UserData::open(tmp.path().join("scratch.db")).unwrap(),
        ));
        fs::remove_file(&path).unwrap();
        b.ud = UserData::open(&path).unwrap();
        assert!(b.ud.snapshot().unwrap().0.is_empty());
        (tmp, a, b)
    }

    fn reset_computer_recovers(n: i64) {
        let (_tmp, mut a, mut b) = reset_setup(n);
        let before = a.ud.snapshot().unwrap();
        // An edit or restore may ask for the own file before the round: it
        // waits, as an empty file would delete everything on A.
        assert!(!b.write());
        let r = b.sync();
        assert!(
            r.iter()
                .all(|r| matches!(r, PeerResult::Merged { deleted: 0, .. })),
            "{r:?}"
        );
        assert_eq!(b.ud.snapshot().unwrap(), before);
        b.write();
        let r = a.sync();
        assert!(
            r.iter()
                .all(|r| matches!(r, PeerResult::Merged { deleted: 0, .. })),
            "{r:?}"
        );
        assert_eq!(a.ud.snapshot().unwrap(), before);
        // Settled: the next rounds do nothing.
        assert!(b.sync().is_empty());
        assert!(a.sync().is_empty());
    }

    #[test]
    fn reset_computer_recovers_everything() {
        reset_computer_recovers(12);
    }

    #[test]
    fn reset_computer_with_few_rows_deletes_nothing_on_the_peer() {
        reset_computer_recovers(3);
    }

    /// A reset B's round in which A's file cannot be merged (`break_it`
    /// spoils it, `mend` undoes that) writes nothing; once the file is
    /// readable B refills and A loses nothing.
    fn reset_round_without_a_merge(break_it: fn(&Path), mend: fn(&Path)) {
        let (tmp, mut a, mut b) = reset_setup(3);
        let before = a.ud.snapshot().unwrap();
        let path = a.folder.dir.join(&a.folder.own);
        let saved = tmp.path().join("saved.jwlibrary");
        fs::copy(&path, &saved).unwrap();
        break_it(&path);
        let r = b.sync();
        assert!(
            r.iter().all(|r| !matches!(r, PeerResult::Merged { .. })),
            "{r:?}"
        );
        assert!(!b.write(), "an empty file would delete A's rows");
        fs::copy(&saved, &path).unwrap();
        mend(&path);
        assert_eq!(merged(&b.sync()), 1);
        assert_eq!(b.ud.snapshot().unwrap(), before);
        assert!(b.write());
        let r = a.sync();
        assert!(
            r.iter()
                .all(|r| matches!(r, PeerResult::Merged { deleted: 0, .. })),
            "{r:?}"
        );
        assert_eq!(a.ud.snapshot().unwrap(), before);
    }

    #[test]
    fn reset_computer_with_a_missing_peer_file_writes_nothing() {
        reset_round_without_a_merge(|p| fs::remove_file(p).unwrap(), |_| {});
    }

    #[test]
    fn reset_computer_with_a_broken_peer_file_writes_nothing() {
        reset_round_without_a_merge(
            |p| {
                let good = fs::read(p).unwrap();
                fs::write(p, &good[..good.len() / 2]).unwrap();
            },
            |_| {},
        );
    }

    #[test]
    fn empty_computer_never_writes_its_file() {
        // No rows, no file: nothing is published. Once it has rows, it
        // writes as usual.
        let (_tmp, mut a, mut b) = setup();
        assert!(!a.write());
        assert!(!a.folder.dir.join(&a.folder.own).exists());
        b.mark(1, 1);
        assert!(b.write());
        let r = a.sync();
        assert_eq!(merged(&r), 1);
        assert!(a.write());
        let r = b.sync();
        assert!(
            r.iter()
                .all(|r| matches!(r, PeerResult::Merged { deleted: 0, .. })),
            "{r:?}"
        );
        assert_eq!(b.ud.snapshot().unwrap().0.len(), 1);
    }

    #[test]
    fn stopped_and_emptied_computer_writes_nothing() {
        // B stopped syncing (keeping its device id), lost its data and its
        // file, and starts again while A's file is unreadable.
        let (tmp, mut a, mut b) = reset_setup(3);
        let before = a.ud.snapshot().unwrap();
        b.folder.forget();
        b.state = SyncState::default();
        fs::remove_file(b.folder.dir.join(&b.folder.own)).unwrap();
        let path = a.folder.dir.join(&a.folder.own);
        let saved = tmp.path().join("saved.jwlibrary");
        fs::copy(&path, &saved).unwrap();
        let good = fs::read(&path).unwrap();
        fs::write(&path, &good[..good.len() / 2]).unwrap();
        let r = b.sync();
        assert!(matches!(&r[..], [PeerResult::Failed { .. }]), "{r:?}");
        assert!(!b.write());
        fs::copy(&saved, &path).unwrap();
        let r = a.sync();
        assert!(
            r.iter()
                .all(|r| matches!(r, PeerResult::Merged { deleted: 0, .. })),
            "{r:?}"
        );
        assert_eq!(a.ud.snapshot().unwrap(), before);
    }

    #[test]
    fn deleting_every_row_publishes_the_empty_file() {
        let (_tmp, mut a, mut b) = setup();
        let guids: Vec<String> = (1..=3).map(|i| a.mark(1, i)).collect();
        assert!(a.write());
        assert_eq!(merged(&b.sync()), 1);
        b.ud.export_backup_as(&b.folder.dir.join(&b.folder.own), "Pergament")
            .unwrap();
        assert_eq!(merged(&a.sync()), 1);
        for g in &guids {
            a.ud.delete_mark(g).unwrap();
        }
        assert!(a.write(), "an intentional delete of every row is published");
        b.sync();
        assert!(b.ud.snapshot().unwrap().0.is_empty());
        assert_eq!(b.ud.snapshot().unwrap(), a.ud.snapshot().unwrap());
    }

    #[test]
    fn empty_computers_do_not_merge_again_and_again() {
        let (_tmp, mut a, mut b) = setup();
        // Empty files, as an older version or another tool may leave them.
        for d in [&a, &b] {
            d.ud.export_backup_as(&d.folder.dir.join(&d.folder.own), "Pergament")
                .unwrap();
        }
        assert_eq!(merged(&a.sync()), 1);
        assert_eq!(merged(&b.sync()), 1);
        // Empty bases hold nothing to refill: the computers are not
        // emptied, and unchanged files are not merged again.
        assert!(a.sync().is_empty());
        assert!(b.sync().is_empty());
        assert!(
            SyncState::load(&a.folder.state_dir)
                .peers
                .contains_key(&b.folder.own)
        );
    }

    #[test]
    fn choice_for_a_peer_that_is_not_pending_is_ignored() {
        let (_tmp, a, mut b, _guids) = mass_delete_setup(10);
        let file = a.folder.own.clone();
        // B has never seen A's deletions and nothing is pending: Apply is
        // ignored and the guard still stops the mass delete.
        let choices = BTreeMap::from([(file, DeleteGuard::Apply)]);
        let r = b.sync_with(&choices);
        assert!(
            matches!(&r[0], PeerResult::NeedsConfirmation { deletions: 10, .. }),
            "{r:?}"
        );
        assert_eq!(b.ud.snapshot().unwrap().0.len(), 12);
    }
}
