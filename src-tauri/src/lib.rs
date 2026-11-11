//! Tauri shell: windows, the updater, folder sync, and the `jwmedia:` image protocol.
//! The portable commands delegate to [`Core`]; logic lives in [`api`] and the `pergament` crate.

pub mod app_update;
pub use pergament_app_core::{api, auto};

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use pergament::catalog::CatalogItem;
use pergament::folder_sync::SyncState;
use pergament::languages::Language;
use pergament::links::{Link, MediaRef};
use pergament::mediator;
use pergament::navigate::{Page, Target};
use pergament::net::{Client, Expected, HttpConfig};
use pergament::remote;
use pergament::sync_snapshot::DeleteGuard;
use pergament::userdata::{Loc, NoteInput, Range};
use pergament_app_core::core::{CacheUsage, ImportResult};
use pergament_app_core::media::{self, MediaRoute};
use pergament_app_core::{Core, Host, Paths};
use serde::Serialize;
use tauri::http::{Response, StatusCode};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use api::{ApiResult, LinkAction, PubCard, PubDetail};
use auto::{SyncMsg, SyncStatus};

/// Sends [`Core`]'s events to the windows, once the app is running.
struct TauriHost(OnceLock<AppHandle>);

impl Host for TauriHost {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        if let Some(app) = self.0.get() {
            let _ = app.emit(event, payload);
        }
    }
}

/// Run `f` with the core on a worker thread, so a slow command does not hold up the others.
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&Core) -> ApiResult<T> + Send + 'static,
) -> ApiResult<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<Core>()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn cache_usage(app: AppHandle) -> ApiResult<CacheUsage> {
    blocking(app, |core| core.cache_usage()).await
}

#[tauri::command]
async fn clear_cache(app: AppHandle) -> ApiResult<()> {
    blocking(app, |core| core.clear_cache()).await
}

#[tauri::command]
fn list_publications(core: State<'_, Core>) -> ApiResult<Vec<PubCard>> {
    core.list_publications()
}

#[tauri::command]
fn publication(core: State<'_, Core>, dir: String) -> ApiResult<PubDetail> {
    core.publication(&dir)
}

#[tauri::command]
fn render_page(core: State<'_, Core>, target: Target) -> ApiResult<Page> {
    core.render_page(&target)
}

#[tauri::command]
fn link_action(
    core: State<'_, Core>,
    current: Option<String>,
    href: String,
) -> ApiResult<LinkAction> {
    core.link_action(current.as_deref(), &href)
}

/// Renditions of a recording, smallest first. The player streams them.
#[tauri::command]
async fn media_links(app: AppHandle, media: MediaRef) -> ApiResult<Vec<remote::MediaFile>> {
    blocking(app, move |core| core.media_links(&media)).await
}

/// Wait before the first due-backup check, so it does not compete with loading the UI.
const FIRST_BACKUP_CHECK_DELAY: Duration = Duration::from_secs(120);

/// A video or audio category in a language, with thumbnails as `jwmedia:` URLs.
/// `detailed` includes the recordings of subcategories.
#[tauri::command]
async fn media_category(
    app: AppHandle,
    lang: String,
    key: String,
    detailed: bool,
) -> ApiResult<mediator::Category> {
    blocking(app, move |core| core.media_category(&lang, &key, detailed)).await
}

/// Download one quality of a recording from a category into the library.
#[tauri::command]
async fn download_media(
    app: AppHandle,
    lang: String,
    category: String,
    detailed: bool,
    key: String,
    label: String,
) -> ApiResult<api::MediaDownload> {
    blocking(app, move |core| {
        core.download_media(&lang, &category, detailed, &key, &label)
    })
    .await
}

#[tauri::command(async)]
fn media_downloads(core: State<'_, Core>) -> ApiResult<Vec<api::MediaDownload>> {
    core.media_downloads()
}

#[tauri::command(async)]
fn remove_media(core: State<'_, Core>, id: i64) -> ApiResult<()> {
    core.remove_media(id)
}

/// Label of the second display window.
const DISPLAY: &str = "display";

/// The year text for the display's idle screen.
#[tauri::command(async)]
fn year_text(
    core: State<'_, Core>,
    lang: String,
    year: i64,
) -> ApiResult<Option<pergament::yeartext::YearText>> {
    core.year_text(&lang, year)
}

/// Open the second display: a black window that is shown fullscreen on a
/// monitor other than the app's, if there is one. Wayland compositors decide
/// where a window goes, so on some setups it has to be moved there by hand.
/// Async, like every command that builds a window: on Windows, building one in
/// a synchronous command deadlocks the main thread and hangs the app.
#[tauri::command(async)]
fn open_display(app: AppHandle) -> ApiResult<()> {
    if let Some(existing) = app.get_webview_window(DISPLAY) {
        return existing.show().map_err(|e| e.to_string());
    }
    let current = app
        .get_webview_window("main")
        .and_then(|w| w.current_monitor().ok().flatten());
    // Without knowing the app's monitor, any monitor could be the app's own.
    let other = current.and_then(|c| {
        app.available_monitors()
            .unwrap_or_default()
            .into_iter()
            .find(|m| c.position() != m.position() || c.size() != m.size())
    });
    let mut builder =
        WebviewWindowBuilder::new(&app, DISPLAY, WebviewUrl::App("index.html".into()))
            .title("Pergament")
            .background_color(tauri::window::Color(0, 0, 0, 255));
    builder = match other {
        Some(monitor) => {
            let at = monitor.position().to_logical::<f64>(monitor.scale_factor());
            builder.position(at.x, at.y).fullscreen(true)
        }
        None => builder.inner_size(1280.0, 720.0),
    };
    builder.build().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn close_display(app: AppHandle) -> ApiResult<()> {
    match app.get_webview_window(DISPLAY) {
        Some(w) => w.close().map_err(|e| e.to_string()),
        None => Ok(()),
    }
}

/// Label of the window notes are written in.
const NOTE: &str = "note";

/// Open the note window, or bring it to the front when it is open. `width` and
/// `height` are the size the user last gave it, in logical pixels.
#[tauri::command(async)]
fn open_note_window(app: AppHandle, width: Option<f64>, height: Option<f64>) -> ApiResult<()> {
    if let Some(existing) = app.get_webview_window(NOTE) {
        existing.unminimize().map_err(|e| e.to_string())?;
        existing.show().map_err(|e| e.to_string())?;
        return existing.set_focus().map_err(|e| e.to_string());
    }
    WebviewWindowBuilder::new(&app, NOTE, WebviewUrl::App("index.html".into()))
        .title("Pergament")
        .inner_size(
            width.unwrap_or(520.0).max(380.0),
            height.unwrap_or(560.0).max(320.0),
        )
        .min_inner_size(380.0, 320.0)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Open an http(s) link in the browser. Anything else is refused.
#[tauri::command]
fn open_external(url: String) -> ApiResult<()> {
    match Link::parse(&url) {
        Some(Link::External(url)) => open::that_detached(url).map_err(|e| e.to_string()),
        _ => Err(format!("refusing to open {url:?}")),
    }
}

#[tauri::command]
fn remove_publication(core: State<'_, Core>, dir: String) -> ApiResult<()> {
    core.remove_publication(&dir)
}

/// Import files on a worker thread with its own library handle.
#[tauri::command]
async fn import_files(app: AppHandle, paths: Vec<String>) -> ApiResult<Vec<ImportResult>> {
    blocking(app, move |core| core.import_files(paths)).await
}

#[tauri::command(async)]
fn book_abbreviations(
    core: State<'_, Core>,
    dir: String,
) -> ApiResult<std::collections::HashMap<i64, String>> {
    core.book_abbreviations(&dir)
}

#[tauri::command(async)]
fn last_verse(core: State<'_, Core>, dir: String, book: i64, chapter: i64) -> ApiResult<i64> {
    core.last_verse(&dir, book, chapter)
}

#[tauri::command(async)]
fn research_verses(
    core: State<'_, Core>,
    dir: String,
    book: i64,
    chapter: i64,
) -> ApiResult<Vec<i64>> {
    core.research_verses(&dir, book, chapter)
}

#[tauri::command(async)]
fn research_guide(
    core: State<'_, Core>,
    dir: String,
    book: i64,
    chapter: i64,
    verse: i64,
) -> ApiResult<Vec<api::ResearchEntry>> {
    core.research_guide(&dir, book, chapter, verse)
}

#[tauri::command(async)]
fn chapter_study(
    core: State<'_, Core>,
    dir: String,
    book: i64,
    chapter: i64,
) -> ApiResult<pergament::render::ChapterStudy> {
    core.chapter_study(&dir, book, chapter)
}

#[tauri::command(async)]
fn book_overview(
    core: State<'_, Core>,
    dir: String,
    book: i64,
) -> ApiResult<pergament::render::BookOverview> {
    core.book_overview(&dir, book)
}

/// Whether a catalog is available without downloading.
#[tauri::command]
async fn catalog_cached(app: AppHandle) -> ApiResult<bool> {
    blocking(app, |core| core.catalog_cached()).await
}

/// Downloaded publications that have a newer version in the catalog; `None` without a catalog.
#[tauri::command]
async fn check_updates(app: AppHandle) -> ApiResult<Option<Vec<api::UpdateInfo>>> {
    blocking(app, |core| core.check_updates()).await
}

/// Download (or refresh) the catalog; reports `progress` events.
#[tauri::command]
async fn load_catalog(app: AppHandle) -> ApiResult<()> {
    blocking(app, |core| core.load_catalog()).await
}

#[tauri::command]
async fn home_lists(
    app: AppHandle,
    lang: String,
    date: String,
) -> ApiResult<Option<api::HomeLists>> {
    blocking(app, move |core| core.home_lists(&lang, &date)).await
}

/// The most recently cataloged publications of the given languages, newest
/// first. `None` when no catalog is cached yet.
#[tauri::command]
async fn whats_new(app: AppHandle, langs: Vec<String>) -> ApiResult<Option<Vec<api::WhatsNewRow>>> {
    blocking(app, move |core| core.whats_new(&langs)).await
}

#[tauri::command]
async fn teaching_toolbox(
    app: AppHandle,
    lang: String,
) -> ApiResult<Option<Vec<api::CatalogEntry>>> {
    blocking(app, move |core| core.teaching_toolbox(&lang)).await
}

#[tauri::command]
async fn categories(app: AppHandle, lang: String) -> ApiResult<Option<Vec<api::Category>>> {
    blocking(app, move |core| core.categories(&lang)).await
}

#[tauri::command]
async fn category(
    app: AppHandle,
    lang: String,
    id: i64,
) -> ApiResult<Option<Vec<api::CatalogEntry>>> {
    blocking(app, move |core| core.category(&lang, id)).await
}

#[tauri::command]
async fn meetings(app: AppHandle, lang: String, date: String) -> ApiResult<Option<api::Meetings>> {
    blocking(app, move |core| core.meetings(&lang, &date)).await
}

#[tauri::command]
async fn dated_page(
    app: AppHandle,
    lang: String,
    kind: String,
    date: String,
) -> ApiResult<Option<api::DatedPage>> {
    blocking(app, move |core| core.dated_page(&lang, &kind, &date)).await
}

#[tauri::command]
async fn referenced_publications(
    app: AppHandle,
    dir: String,
    lang: String,
) -> ApiResult<Option<Vec<api::LanguageEntry>>> {
    blocking(app, move |core| core.referenced_publications(&dir, &lang)).await
}

#[tauri::command]
async fn missing_entry(
    app: AppHandle,
    lang: String,
    href: String,
) -> ApiResult<Option<api::CatalogEntry>> {
    blocking(app, move |core| core.missing_entry(&lang, &href)).await
}

#[tauri::command]
fn page_user_data(core: State<'_, Core>, target: Target) -> ApiResult<api::PageUserData> {
    core.page_user_data(&target)
}

/// Keep the text of an answer field of a page.
#[tauri::command]
fn save_answer(core: State<'_, Core>, target: Target, tag: String, value: String) -> ApiResult<()> {
    core.save_answer(&target, &tag, &value)
}

#[tauri::command]
fn add_mark(
    core: State<'_, Core>,
    target: Target,
    color: i64,
    ranges: Vec<Range>,
) -> ApiResult<String> {
    core.add_mark(&target, color, &ranges)
}

#[tauri::command]
fn set_mark_color(core: State<'_, Core>, guid: String, color: i64) -> ApiResult<()> {
    core.set_mark_color(&guid, color)
}

#[tauri::command]
fn delete_mark(core: State<'_, Core>, guid: String) -> ApiResult<()> {
    core.delete_mark(&guid)
}

/// Create (with `target`) or update a note; returns its guid.
#[tauri::command]
fn save_note(core: State<'_, Core>, target: Option<Target>, note: NoteInput) -> ApiResult<String> {
    core.save_note(target.as_ref(), &note)
}

#[tauri::command]
fn delete_note(core: State<'_, Core>, guid: String) -> ApiResult<()> {
    core.delete_note(&guid)
}

#[tauri::command]
fn all_notes(core: State<'_, Core>, tag: Option<i64>) -> ApiResult<Vec<pergament::userdata::Note>> {
    core.all_notes(tag)
}

#[tauri::command]
fn tags(core: State<'_, Core>) -> ApiResult<Vec<pergament::userdata::TagInfo>> {
    core.tags()
}

#[tauri::command]
fn bookmarks(core: State<'_, Core>) -> ApiResult<Vec<pergament::userdata::Bookmark>> {
    core.bookmarks()
}

#[tauri::command]
fn user_data_summary(core: State<'_, Core>) -> ApiResult<pergament::userdata::Summary> {
    core.user_data_summary()
}

#[tauri::command]
fn open_location(core: State<'_, Core>, loc: Loc) -> ApiResult<Option<Target>> {
    core.open_location(&loc)
}

#[tauri::command]
async fn export_backup(app: AppHandle, path: String) -> ApiResult<()> {
    blocking(app, move |core| core.export_backup(Path::new(&path))).await
}

/// Replace all user data with a `.jwlibrary` backup, after a rolling backup of the current data.
#[tauri::command]
async fn restore_backup(app: AppHandle, path: String) -> ApiResult<pergament::userdata::Summary> {
    blocking(app, move |core| core.restore_backup(Path::new(&path))).await
}

/// Merge a `.jwlibrary` backup into the user data instead of replacing it.
#[tauri::command]
async fn merge_backup(app: AppHandle, path: String) -> ApiResult<pergament::userdata::MergeReport> {
    blocking(app, move |core| core.merge_backup(Path::new(&path))).await
}

/// Language list (code, names), cached for a week.
#[tauri::command]
async fn languages(app: AppHandle) -> ApiResult<Vec<Language>> {
    blocking(app, |core| core.languages()).await
}

#[tauri::command]
async fn catalog_search(
    app: AppHandle,
    lang: String,
    query: String,
) -> ApiResult<Vec<api::CatalogEntry>> {
    blocking(app, move |core| core.catalog_search(&lang, &query)).await
}

/// The publication in every catalog language; `None` when no catalog is cached.
#[tauri::command]
async fn catalog_languages(
    app: AppHandle,
    symbol: String,
    issue_tag: i64,
) -> ApiResult<Option<Vec<api::LanguageEntry>>> {
    blocking(app, move |core| core.catalog_languages(&symbol, issue_tag)).await
}

/// Catalog entries of favorite keys; `None` when no catalog is cached.
#[tauri::command]
async fn favorite_entries(
    app: AppHandle,
    lang: String,
    keys: Vec<String>,
) -> ApiResult<Option<Vec<api::CatalogEntry>>> {
    blocking(app, move |core| core.favorite_entries(&lang, &keys)).await
}

#[tauri::command]
async fn download_publication(
    app: AppHandle,
    item: CatalogItem,
    lang: String,
) -> ApiResult<String> {
    blocking(app, move |core| core.download_publication(&item, &lang)).await
}

/// A client for GitHub's API and release downloads, which redirect to githubusercontent.com.
fn github_client() -> Client {
    Client::new(HttpConfig {
        allowed_hosts: vec!["github.com".to_owned(), "githubusercontent.com".to_owned()],
        ..HttpConfig::default()
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateCheck {
    kind: &'static str,
    available: Option<app_update::Available>,
}

/// Look for a newer release. Nix builds never ask GitHub.
#[tauri::command]
async fn check_app_update(app: AppHandle) -> ApiResult<UpdateCheck> {
    tauri::async_runtime::spawn_blocking(move || {
        let kind = app_update::current_install_kind();
        let name = match kind {
            app_update::InstallKind::Nix => {
                return Ok(UpdateCheck {
                    kind: "nix",
                    available: None,
                });
            }
            app_update::InstallKind::AppImage(_) => "appimage",
            app_update::InstallKind::Zip { .. } => "zip",
            app_update::InstallKind::Manual => "manual",
        };
        let release: app_update::Release = github_client()
            .get_json(app_update::RELEASES_URL)
            .map_err(|e| e.to_string())?;
        let running = app.package_info().version.to_string();
        Ok(UpdateCheck {
            kind: name,
            available: app_update::available(&release, &running, &kind, std::env::consts::ARCH),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Download `version`, check its signature and put it in place. The release is
/// fetched again here, so nothing the page sends decides what is installed.
#[tauri::command]
async fn install_app_update(app: AppHandle, version: String) -> ApiResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let kind = app_update::current_install_kind();
        let client = github_client();
        let release: app_update::Release = client
            .get_json(app_update::RELEASES_URL)
            .map_err(|e| e.to_string())?;
        if release.tag_name.strip_prefix('v') != Some(version.as_str()) {
            return Err(format!(
                "the latest release is {}, not {version}",
                release.tag_name
            ));
        }
        let (file, sig) = app_update::assets_for(&release, &kind, std::env::consts::ARCH)
            .ok_or("this installation cannot be updated from the app")?;
        let dir = app.state::<Core>().cache().join("app-update");
        let file_path = dir.join(&file.name);
        let sig_path = dir.join(&sig.name);
        let result = (|| {
            client
                .download(
                    &sig.browser_download_url,
                    &sig_path,
                    &Expected::default(),
                    &mut |_, _| {},
                )
                .map_err(|e| e.to_string())?;
            client
                .download(
                    &file.browser_download_url,
                    &file_path,
                    &Expected::default(),
                    &mut app.state::<Core>().progress("app-update"),
                )
                .map_err(|e| e.to_string())?;
            let sig_text = std::fs::read_to_string(&sig_path).map_err(|e| e.to_string())?;
            app_update::verify_file(&file_path, &file.name, &sig_text, app_update::PUBLIC_KEY)?;
            app_update::install(&kind, &file_path)
        })();
        let _ = std::fs::remove_dir_all(&dir);
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Downloads and imports that are still running.
#[tauri::command]
fn running_jobs(core: State<'_, Core>) -> usize {
    core.running_jobs()
}

/// Start the installed (possibly just replaced) app again.
#[tauri::command]
fn restart_app(app: AppHandle) {
    // Goes through the exit events, so the close backup runs before the restart.
    app.request_restart();
}

/// Whether serving `path` requires a network fetch (an uncached remote image).
fn needs_fetch(app: &AppHandle, path: &str) -> bool {
    media::needs_fetch(&app.state::<Core>(), path)
}

/// A picture, whole and with the two headers this protocol has always sent.
fn media_response(app: &AppHandle, path: &str) -> Response<Vec<u8>> {
    let mut picture = media::media_response(&app.state::<Core>(), MediaRoute::Picture, path);
    let mut bytes = Vec::new();
    let read = picture.status == 200 && picture.body.read_to_end(&mut bytes).is_ok();
    match (read, picture.header("Content-Type")) {
        (true, Some(mime)) => Response::builder()
            .header("Content-Type", mime)
            .header("Cache-Control", "max-age=3600")
            .body(bytes)
            .unwrap_or_default(),
        _ => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap_or_default(),
    }
}

/// Bytes of a `jwmedia:` URL. The frontend loads images through this and shows them
/// as blob URLs, which WebKit allows from any page origin (it refuses the
/// custom scheme from the dev server's `http://localhost`).
#[tauri::command]
async fn media(app: AppHandle, url: String) -> ApiResult<tauri::ipc::Response> {
    let prefix = format!("{}://localhost", api::MEDIA_SCHEME);
    let path = url
        .strip_prefix(&prefix)
        .ok_or_else(|| "not a media URL".to_string())?
        .to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let response = media_response(&app, &path);
        if response.status() == StatusCode::OK {
            Ok(tauri::ipc::Response::new(response.into_body()))
        } else {
            Err(format!("no image at {path}"))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// What the sync worker collected from its messages and timers.
#[derive(Default)]
struct SyncWork {
    write: bool,
    sync: bool,
    reconfigure: bool,
    choices: BTreeMap<String, DeleteGuard>,
    /// Dropped once the work is done, which wakes the waiting commands.
    done: Vec<Sender<()>>,
}

impl SyncWork {
    fn add(&mut self, msg: SyncMsg, auto: &auto::Auto) {
        match msg {
            SyncMsg::Wake => {}
            SyncMsg::FolderChanged => {
                if let Ok(mut s) = auto.schedule.lock() {
                    s.folder_changed(Instant::now());
                }
            }
            SyncMsg::WriteNow => self.write = true,
            SyncMsg::Reconfigure => self.reconfigure = true,
            SyncMsg::Sync { choices, done } => {
                self.sync = true;
                self.choices.extend(choices);
                self.done.extend(done);
            }
        }
    }
}

/// The folder being watched, and its watcher when watching works there.
type Watched = (PathBuf, Option<notify::RecommendedWatcher>);

/// The one thread that writes this computer's sync file and merges the
/// others, so merges never overlap. It holds the user data lock only while
/// writing or merging, never while it waits.
fn sync_worker(app: AppHandle, tx: Sender<SyncMsg>, rx: Receiver<SyncMsg>) {
    let mut watched: Option<Watched> = None;
    let mut health = auto::FolderHealth::default();
    loop {
        let state = app.state::<Core>();
        let auto = state.auto();
        let deadline = auto.schedule.lock().ok().and_then(|s| s.next_deadline());
        let first = match deadline {
            None => match rx.recv() {
                Ok(msg) => Some(msg),
                Err(_) => return,
            },
            Some(at) => match rx.recv_timeout(at.saturating_duration_since(Instant::now())) {
                Ok(msg) => Some(msg),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            },
        };
        let mut work = SyncWork::default();
        for msg in first.into_iter().chain(rx.try_iter()) {
            work.add(msg, auto);
        }
        if let Ok(mut s) = auto.schedule.lock() {
            let due = s.take_due(Instant::now());
            work.write |= due.write;
            work.sync |= due.sync;
        }
        update_watcher(auto, &tx, &mut watched, work.reconfigure);
        if work.sync {
            let readable = sync_round(&app, &work.choices);
            if readable.is_some_and(|r| health.round(r)) {
                update_watcher(auto, &tx, &mut watched, true);
            }
        } else if work.write {
            sync_write(&app);
        }
    }
}

/// Watch the configured folder; a new folder, or `force`, starts over.
fn update_watcher(
    auto: &auto::Auto,
    tx: &Sender<SyncMsg>,
    watched: &mut Option<Watched>,
    force: bool,
) {
    let folder = auto.sync_folder();
    let wanted = folder.as_ref().map(|f| &f.dir);
    if !force && watched.as_ref().map(|(dir, _)| dir) == wanted {
        return;
    }
    *watched = folder.map(|f| {
        let watcher = watch_folder(&f.dir, f.own.clone(), tx.clone())
            .inspect_err(|e| eprintln!("cannot watch {}: {e}", f.dir.display()))
            .ok();
        (f.dir, watcher)
    });
}

/// Report changes to other computers' files in `dir`; the poll covers the rest.
fn watch_folder(
    dir: &Path,
    own: String,
    tx: Sender<SyncMsg>,
) -> notify::Result<notify::RecommendedWatcher> {
    use notify::event::{AccessKind, AccessMode, EventKind};
    use notify::{RecursiveMode, Watcher};
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let relevant = match event {
            // Reading the files (as a merge does) changes nothing.
            Ok(event) if matches!(event.kind, EventKind::Access(kind) if kind != AccessKind::Close(AccessMode::Write)) => {
                false
            }
            Ok(event) => {
                event.paths.is_empty() || event.paths.iter().any(|p| is_peer_file(p, &own))
            }
            Err(_) => true,
        };
        if relevant {
            let _ = tx.send(SyncMsg::FolderChanged);
        }
    })?;
    watcher.watch(dir, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}

/// Another computer's `Pergament-<host>-<id>.jwlibrary`; temp files start
/// with a dot, and rolling backups are no device files.
fn is_peer_file(path: &Path, own: &str) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n != own && pergament::folder_sync::is_device_file(n))
}

/// Merge the other computers' files, then write this computer's file.
/// Returns whether the folder could be read; `None` when no round ran.
fn sync_round(app: &AppHandle, choices: &BTreeMap<String, DeleteGuard>) -> Option<bool> {
    let state = app.state::<Core>();
    let auto = state.auto();
    // Sync off: leave the user data alone (checked again under the lock).
    auto.sync_folder()?;
    let no_choices = BTreeMap::new();
    let (merged, written, sync_state, backup_error) = {
        let _job = state.job();
        let mut user = match state.user() {
            Ok(user) => user,
            Err(e) => {
                eprintln!("sync failed: {e}");
                return None;
            }
        };
        // Under the user lock the folder cannot change, and after the close
        // write nothing runs any more.
        if auto.schedule.lock().map_or(true, |s| s.is_closed()) {
            return None;
        }
        let folder = auto.sync_folder()?;
        // Applied deletions go only after a rolling backup of the data as
        // they are; without one the choices wait for the next round.
        let mut choices = choices;
        let mut backup_error = None;
        if auto::needs_safety_backup(choices)
            && let Err(e) = auto.write_safety_backup(&user)
        {
            eprintln!("rolling backup before sync deletions failed: {e}");
            backup_error = Some(format!(
                "cannot back up the data before applying the deletions: {e}"
            ));
            choices = &no_choices;
        }
        let mut sync_state = SyncState::load(&folder.state_dir);
        let merged = folder.sync_peers(&mut user, &mut sync_state, choices);
        // An unreadable folder changes nothing, the own file included.
        let written = merged.is_ok().then(|| {
            if let Ok(mut s) = auto.schedule.lock() {
                s.begin_write();
            }
            folder.write_own(&user, auto.host(), &mut sync_state)
        });
        (merged, written, sync_state, backup_error)
    };
    let readable = merged.is_ok();
    let mut changed = false;
    if let Ok(mut status) = auto.status.lock() {
        match merged {
            Ok(results) => changed = status.record(results, auto::unix_now()),
            Err(e) => status.error = Some(format!("cannot read the sync folder: {e}")),
        }
        status.sync_pending(&sync_state);
        if let Some(Err(e)) = &written {
            status.error = Some(format!("cannot write to the sync folder: {e}"));
        }
        if let Some(e) = backup_error {
            status.add_error(e);
        }
    }
    if let Ok(mut s) = auto.schedule.lock() {
        if matches!(written, Some(Err(_))) {
            s.write_failed();
        }
        s.synced(Instant::now());
    }
    if changed {
        let _ = app.emit_to("main", "user-data-changed", ());
    }
    Some(readable)
}

/// Write this computer's file after edits or a restore.
fn sync_write(app: &AppHandle) {
    let state = app.state::<Core>();
    let auto = state.auto();
    // Sync off: leave the user data alone (checked again under the lock).
    if auto.sync_folder().is_none() {
        return;
    }
    let written = {
        let _job = state.job();
        let user = match state.user() {
            Ok(user) => user,
            Err(e) => return eprintln!("sync write failed: {e}"),
        };
        if auto.schedule.lock().map_or(true, |s| s.is_closed()) {
            return;
        }
        let Some(folder) = auto.sync_folder() else {
            return;
        };
        if let Ok(mut s) = auto.schedule.lock() {
            s.begin_write();
        }
        let mut sync_state = SyncState::load(&folder.state_dir);
        folder.write_own(&user, auto.host(), &mut sync_state)
    };
    if let Err(e) = written {
        eprintln!("sync write failed: {e}");
        if let Ok(mut s) = auto.schedule.lock() {
            s.write_failed();
        }
        if let Ok(mut status) = auto.status.lock() {
            status.error = Some(format!("cannot write to the sync folder: {e}"));
        }
    }
}

/// Have the worker run a merge round and wait until it is over.
fn sync_and_wait(auto: &auto::Auto, choices: BTreeMap<String, DeleteGuard>) {
    let (done, wait) = mpsc::channel();
    auto.send(SyncMsg::Sync {
        choices,
        done: Some(done),
    });
    // The worker drops `done` when the round is over.
    let _ = wait.recv();
}

/// Sync with `path`, or stop syncing with `None` (the folder's files stay).
#[tauri::command]
async fn set_sync_folder(app: AppHandle, path: Option<String>) -> ApiResult<auto::AutoSettings> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Core>();
        let path = path.map(PathBuf::from);
        if path.as_ref().is_some_and(|p| !p.is_dir()) {
            return Err("not a folder".to_owned());
        }
        let settings = {
            // No merge runs while the folder changes.
            let _job = state.job();
            let _user = state.user()?;
            state
                .auto()
                .set_sync_folder(path)
                .map_err(|e| e.to_string())?
        };
        state.auto().send(SyncMsg::Reconfigure);
        if settings.sync.folder.is_some() {
            sync_and_wait(state.auto(), BTreeMap::new());
        }
        Ok(settings)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn sync_now(app: AppHandle) -> ApiResult<SyncStatus> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Core>();
        sync_and_wait(state.auto(), BTreeMap::new());
        Ok(state.auto().sync_status())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn sync_status(state: State<'_, Core>) -> SyncStatus {
    state.auto().sync_status()
}

/// Answer a held-back peer: `apply` its deletions, or keep the data here.
#[tauri::command]
async fn resolve_peer(app: AppHandle, file: String, apply: bool) -> ApiResult<SyncStatus> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Core>();
        let guard = if apply {
            DeleteGuard::Apply
        } else {
            DeleteGuard::Skip
        };
        sync_and_wait(state.auto(), BTreeMap::from([(file, guard)]));
        Ok(state.auto().sync_status())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn auto_settings(state: State<'_, Core>) -> auto::AutoSettings {
    state.auto().settings()
}

#[tauri::command]
fn set_rolling(
    state: State<'_, Core>,
    enabled: bool,
    keep: usize,
) -> ApiResult<auto::AutoSettings> {
    state
        .auto()
        .set_rolling(enabled, keep)
        .map_err(|e| e.to_string())
}

/// Show the rolling backups folder in the file manager.
#[tauri::command]
fn open_backups_folder(state: State<'_, Core>) -> ApiResult<()> {
    let dir = state.auto().rolling_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    open::that_detached(&dir).map_err(|e| e.to_string())
}

pub fn run() {
    // Tauri restarts `$APPIMAGE` when it is set; one inherited from another AppImage is not this app.
    if app_update::stray_appimage() {
        // SAFETY: no other threads exist yet.
        unsafe { std::env::remove_var("APPIMAGE") };
    }
    let root = std::env::var_os("PERGAMENT_LIBRARY")
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|d| pergament::library::app_dir(&d)))
        .expect("no data directory (set XDG_DATA_HOME or PERGAMENT_LIBRARY)");
    let cache = std::env::var_os("PERGAMENT_CACHE")
        .map(PathBuf::from)
        .or_else(|| dirs::cache_dir().map(|d| pergament::library::app_dir(&d)))
        .expect("no cache directory (set XDG_CACHE_HOME or PERGAMENT_CACHE)");
    let host = Arc::new(TauriHost(OnceLock::new()));
    let core = Core::open(
        Paths {
            library: root,
            cache,
        },
        host.clone(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let media_dir = core.media_dir().expect("cannot find the media directory");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(core)
        .on_window_event(|window, event| {
            let app = window.app_handle();
            if matches!(event, WindowEvent::Focused(true)) && window.label() == "main" {
                // Back at the app: bring in what the other computers did.
                app.state::<Core>().auto().send(SyncMsg::Sync {
                    choices: BTreeMap::new(),
                    done: None,
                });
                return;
            }
            if !matches!(event, WindowEvent::Destroyed) {
                return;
            }
            match window.label() {
                // Tell the app the display was closed, so the setting follows.
                DISPLAY => {
                    let _ = app.emit_to("main", "display-closed", ());
                }
                // The app forgets the note it sent there.
                NOTE => {
                    let _ = app.emit_to("main", "note-closed", ());
                }
                // Neither window outlives the app window. The app already asked
                // about unsaved text, so the note window goes without asking again.
                "main" => {
                    if let Some(display) = app.get_webview_window(DISPLAY) {
                        let _ = display.close();
                    }
                    if let Some(note) = app.get_webview_window(NOTE) {
                        let _ = note.destroy();
                    }
                    // Back up the data on the way out; it is quick and errors only get logged.
                    app.state::<Core>().backup_on_close();
                }
                _ => {}
            }
        })
        .setup(move |app| {
            // From here on, the core's events reach the windows.
            let _ = host.0.set(app.handle().clone());
            // Downloaded recordings are played through the asset protocol, which
            // supports range requests; only the media folder is reachable.
            app.asset_protocol_scope()
                .allow_directory(&media_dir, true)?;
            // Once an hour, write the daily rolling backup when it is due.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                // Stay out of the way while the window loads the user data.
                std::thread::sleep(FIRST_BACKUP_CHECK_DELAY);
                loop {
                    handle.state::<Core>().backup_rolling(true);
                    std::thread::sleep(Duration::from_secs(3600));
                }
            });
            // Folder sync: one worker, starting with a round at startup.
            let (tx, rx) = mpsc::channel();
            let auto = app.state::<Core>().inner().auto();
            auto.set_worker(tx.clone());
            auto.send(SyncMsg::Sync {
                choices: BTreeMap::new(),
                done: None,
            });
            let handle = app.handle().clone();
            std::thread::spawn(move || sync_worker(handle, tx, rx));
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol(api::MEDIA_SCHEME, |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let path = request.uri().path().to_owned();
            if needs_fetch(&app, &path) {
                std::thread::spawn(move || responder.respond(media_response(&app, &path)));
            } else {
                responder.respond(media_response(&app, &path));
            }
        })
        .invoke_handler(tauri::generate_handler![
            media,
            cache_usage,
            clear_cache,
            list_publications,
            publication,
            render_page,
            link_action,
            open_external,
            check_app_update,
            install_app_update,
            running_jobs,
            restart_app,
            media_links,
            year_text,
            open_display,
            close_display,
            open_note_window,
            media_category,
            download_media,
            media_downloads,
            remove_media,
            remove_publication,
            import_files,
            catalog_search,
            catalog_languages,
            favorite_entries,
            languages,
            chapter_study,
            book_overview,
            book_abbreviations,
            last_verse,
            research_verses,
            research_guide,
            catalog_cached,
            load_catalog,
            check_updates,
            home_lists,
            teaching_toolbox,
            whats_new,
            categories,
            category,
            meetings,
            download_publication,
            dated_page,
            missing_entry,
            referenced_publications,
            page_user_data,
            save_answer,
            add_mark,
            set_mark_color,
            delete_mark,
            save_note,
            delete_note,
            all_notes,
            tags,
            bookmarks,
            user_data_summary,
            open_location,
            export_backup,
            restore_backup,
            merge_backup,
            auto_settings,
            set_rolling,
            open_backups_folder,
            set_sync_folder,
            sync_now,
            sync_status,
            resolve_peer,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Pergament")
        .run(|app, event| {
            // Cmd+Q on macOS and restarts skip the window events; Exit is always sent.
            if matches!(event, tauri::RunEvent::Exit) {
                app.state::<Core>().backup_on_close();
            }
        });
}

#[cfg(test)]
mod tests {
    /// The watcher ignores this computer's own writes, so they don't start a merge.
    #[test]
    fn watcher_reacts_to_other_computers_files_only() {
        use super::is_peer_file;
        use std::path::Path;
        let own = "Pergament-me-000001.jwlibrary";
        assert!(is_peer_file(
            Path::new("/s/Pergament-you-000002.jwlibrary"),
            own
        ));
        assert!(!is_peer_file(
            Path::new("/s/Pergament-me-000001.jwlibrary"),
            own
        ));
        assert!(!is_peer_file(
            Path::new("/s/.Pergament-me-000001.jwlibrary.part"),
            own
        ));
        assert!(!is_peer_file(Path::new("/s/backup.jwlibrary"), own));
        assert!(!is_peer_file(
            Path::new("/s/Pergament-2026-10-20-142501.jwlibrary"),
            own
        ));
    }

    /// The windows close themselves with `destroy`, so that a close request does not ask twice.
    #[test]
    fn main_and_note_windows_may_destroy_themselves() {
        for (name, json) in [
            ("main", include_str!("../capabilities/main.json")),
            ("note", include_str!("../capabilities/note.json")),
        ] {
            let capability: serde_json::Value = serde_json::from_str(json).unwrap();
            let allowed = capability["permissions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p == "core:window:allow-destroy");
            assert!(allowed, "{name}.json lacks core:window:allow-destroy");
        }
    }
}
