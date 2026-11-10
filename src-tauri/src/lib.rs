//! Tauri shell: managed state, commands and the `jwmedia:` image protocol.
//! Logic lives in [`api`] and the `pergament` crate.

pub mod api;
pub mod app_update;
pub mod auto;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard};
use std::time::{Duration, Instant};

use pergament::Library;
use pergament::catalog::{Catalog, CatalogItem};
use pergament::folder_sync::SyncState;
use pergament::languages::Language;
use pergament::links::{Link, MediaRef};
use pergament::mediator;
use pergament::navigate::{Page, Target};
use pergament::net::{Client, Expected, HttpConfig};
use pergament::remote::{self, Request};
use pergament::sync_snapshot::DeleteGuard;
use pergament::userdata::{self, Loc, NoteInput, Range, UserData};
use serde::Serialize;
use tauri::http::{Response, StatusCode};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use api::{ApiResult, LinkAction, PubCard, PubDetail};
use auto::{SyncMsg, SyncStatus};

pub struct AppState {
    library: Mutex<Library>,
    /// Highlights, notes and tags (`userData.db` in the library root).
    user: Mutex<UserData>,
    root: PathBuf,
    cache: PathBuf,
    catalog: Mutex<Option<Arc<Mutex<Catalog>>>>,
    /// Held shared by everything that opens or uses the cached catalog, and exclusively while the
    /// cache is cleared, so its files are never deleted from under a live connection.
    cache_gate: RwLock<()>,
    /// Shared client for catalog images, so their requests are rate limited together.
    image_client: Mutex<Client>,
    /// Downloads and imports running now; an app update waits for them before it restarts.
    jobs: AtomicUsize,
    /// Automatic backup settings and state.
    auto: auto::Auto,
}

/// Counts a running job in [`AppState::jobs`] until it drops.
struct JobGuard<'a>(&'a AtomicUsize);

impl<'a> JobGuard<'a> {
    fn new(jobs: &'a AtomicUsize) -> Self {
        jobs.fetch_add(1, Ordering::SeqCst);
        Self(jobs)
    }
}

impl Drop for JobGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl AppState {
    fn lib(&self) -> ApiResult<std::sync::MutexGuard<'_, Library>> {
        self.library
            .lock()
            .map_err(|_| "library lock poisoned".to_owned())
    }

    fn user(&self) -> ApiResult<std::sync::MutexGuard<'_, UserData>> {
        self.user
            .lock()
            .map_err(|_| "user data lock poisoned".to_owned())
    }

    /// Count a running job until the guard drops; an app update waits for it before restarting.
    fn job(&self) -> JobGuard<'_> {
        JobGuard::new(&self.jobs)
    }

    /// Use the cached catalog until the guard drops; clearing the cache waits for it.
    fn cache_use(&self) -> ApiResult<RwLockReadGuard<'_, ()>> {
        self.cache_gate
            .read()
            .map_err(|_| "cache lock poisoned".to_owned())
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    task: String,
    done: u64,
    total: Option<u64>,
}

fn progress(app: &AppHandle, task: &str) -> impl FnMut(u64, Option<u64>) + use<> {
    let app = app.clone();
    let task = task.to_owned();
    let mut last = u64::MAX;
    move |done, total| {
        // Emit roughly once per percent (or MiB without a total).
        let key = total.map_or(done >> 20, |t| done * 100 / t.max(1));
        if key != last {
            last = key;
            let _ = app.emit(
                "progress",
                Progress {
                    task: task.clone(),
                    done,
                    total,
                },
            );
        }
    }
}

#[derive(Serialize)]
struct CacheUsage {
    #[serde(flatten)]
    usage: pergament::cache::Usage,
    total: u64,
}

#[tauri::command]
async fn cache_usage(app: AppHandle) -> ApiResult<CacheUsage> {
    tauri::async_runtime::spawn_blocking(move || {
        let usage =
            pergament::cache::usage(&app.state::<AppState>().cache).map_err(|e| e.to_string())?;
        Ok(CacheUsage {
            total: usage.total(),
            usage,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn clear_cache(app: AppHandle) -> ApiResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        // PERGAMENT_CACHE may point anywhere; never empty the library with it.
        pergament::cache::check_safe(&state.cache, &state.root).map_err(|e| e.to_string())?;
        // Refuse while a download or catalog load is using the cache, instead of waiting: a
        // waiting writer would hold up every new catalog request behind it.
        let _gate = match state.cache_gate.try_write() {
            Ok(gate) => gate,
            Err(std::sync::TryLockError::WouldBlock) => {
                return Err("The cache is in use by a download or a catalog load. Try again when it has finished.".to_owned());
            }
            Err(_) => return Err("cache lock poisoned".to_owned()),
        };
        // Release the open catalog; it reloads on next use.
        *state.catalog.lock().map_err(|e| e.to_string())? = None;
        pergament::cache::clear(&state.cache).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn list_publications(state: State<'_, AppState>) -> ApiResult<Vec<PubCard>> {
    api::list_publications(&*state.lib()?)
}

#[tauri::command]
fn publication(state: State<'_, AppState>, dir: String) -> ApiResult<PubDetail> {
    api::publication(&*state.lib()?, &dir)
}

#[tauri::command]
fn render_page(state: State<'_, AppState>, target: Target) -> ApiResult<Page> {
    api::render(&*state.lib()?, &target)
}

#[tauri::command]
fn link_action(
    state: State<'_, AppState>,
    current: Option<String>,
    href: String,
) -> ApiResult<LinkAction> {
    api::link_action(&*state.lib()?, current.as_deref(), &href)
}

/// Renditions of a recording, smallest first. The player streams them.
#[tauri::command]
async fn media_links(media: MediaRef) -> ApiResult<Vec<remote::MediaFile>> {
    tauri::async_runtime::spawn_blocking(move || {
        let client = Client::new(HttpConfig::default());
        remote::media_links(&client, &media).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// How long a cached media category is used before it is fetched again.
/// Wait before the first due-backup check, so it does not compete with loading the UI.
const FIRST_BACKUP_CHECK_DELAY: Duration = Duration::from_secs(120);

const MEDIA_CATEGORY_TTL: Duration = Duration::from_secs(6 * 3600);

/// A video or audio category in a language, with thumbnails as `jwmedia:` URLs.
/// `detailed` includes the recordings of subcategories.
#[tauri::command]
async fn media_category(
    app: AppHandle,
    lang: String,
    key: String,
    detailed: bool,
) -> ApiResult<mediator::Category> {
    tauri::async_runtime::spawn_blocking(move || {
        let client = Client::new(HttpConfig::default());
        let cache = app.state::<AppState>().cache.clone();
        let mut category =
            mediator::category(&client, &cache, &lang, &key, detailed, MEDIA_CATEGORY_TTL)
                .map_err(|e| e.to_string())?;
        category.map_images(&api::cms_image_url);
        Ok(category)
    })
    .await
    .map_err(|e| e.to_string())?
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
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _job = state.job();
        let _gate = state.cache_use()?;
        let client = Client::new(HttpConfig::default());
        let found = mediator::category(
            &client,
            &state.cache,
            &lang,
            &category,
            detailed,
            MEDIA_CATEGORY_TTL,
        )
        .map_err(|e| e.to_string())?;
        let media = found
            .find(&key)
            .ok_or_else(|| format!("{key} is not in {category}"))?;
        // A separate handle, so rendering is not blocked while the file downloads.
        let lib = Library::open(&state.root).map_err(|e| e.to_string())?;
        let entry = remote::download_media(
            &client,
            &lib,
            &lang,
            media,
            &label,
            &mut progress(&app, &format!("media:{key}")),
        )
        .map_err(|e| e.to_string())?;
        Ok(api::media_download(&lib, entry))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(async)]
fn media_downloads(state: State<'_, AppState>) -> ApiResult<Vec<api::MediaDownload>> {
    let lib = state.lib()?;
    let list = lib.list_media().map_err(|e| e.to_string())?;
    Ok(list
        .into_iter()
        .map(|e| api::media_download(&lib, e))
        .collect())
}

#[tauri::command(async)]
fn remove_media(state: State<'_, AppState>, id: i64) -> ApiResult<()> {
    state.lib()?.remove_media(id).map_err(|e| e.to_string())?;
    Ok(())
}

/// Label of the second display window.
const DISPLAY: &str = "display";

/// The year text for the display's idle screen.
#[tauri::command(async)]
fn year_text(
    state: State<'_, AppState>,
    lang: String,
    year: i64,
) -> ApiResult<Option<pergament::yeartext::YearText>> {
    api::year_text(&*state.lib()?, &lang, year)
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
fn remove_publication(state: State<'_, AppState>, dir: String) -> ApiResult<()> {
    let mut lib = state.lib()?;
    let entry = lib
        .get_by_dir(&dir)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("{dir} is not in the library"))?;
    lib.remove(&entry).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportResult {
    path: String,
    title: Option<String>,
    error: Option<String>,
}

/// Import files on a worker thread with its own library handle.
#[tauri::command]
async fn import_files(app: AppHandle, paths: Vec<String>) -> ApiResult<Vec<ImportResult>> {
    let root = app.state::<AppState>().root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _job = state.job();
        let mut lib = Library::open(&root).map_err(|e| e.to_string())?;
        Ok(paths
            .into_iter()
            .map(|path| match lib.import(&path) {
                Ok(e) => ImportResult {
                    path,
                    title: Some(e.title),
                    error: None,
                },
                Err(e) => ImportResult {
                    path,
                    title: None,
                    error: Some(e.to_string()),
                },
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn catalog(app: &AppHandle) -> ApiResult<Arc<Mutex<Catalog>>> {
    let state = app.state::<AppState>();
    let _gate = state.cache_use()?;
    catalog_unguarded(app)
}

/// [`catalog`] for callers that already hold the cache gate.
fn catalog_unguarded(app: &AppHandle) -> ApiResult<Arc<Mutex<Catalog>>> {
    let state = app.state::<AppState>();
    if let Some(c) = state.catalog.lock().map_err(|e| e.to_string())?.clone() {
        return Ok(c);
    }
    let _job = state.job();
    let client = Client::new(HttpConfig::default());
    let catalog = Catalog::load(
        &client,
        &state.cache,
        false,
        Duration::from_secs(24 * 3600),
        &mut progress(app, "catalog"),
    )
    .map_err(|e| e.to_string())?;
    let catalog = Arc::new(Mutex::new(catalog));
    *state.catalog.lock().map_err(|e| e.to_string())? = Some(catalog.clone());
    Ok(catalog)
}

/// The catalog if it is loaded or cached; never downloads.
fn cached_catalog(app: &AppHandle) -> ApiResult<Option<Arc<Mutex<Catalog>>>> {
    let state = app.state::<AppState>();
    let mut slot = state.catalog.lock().map_err(|e| e.to_string())?;
    if slot.is_none()
        && let Some(c) = Catalog::open_cached(&state.cache).map_err(|e| e.to_string())?
    {
        *slot = Some(Arc::new(Mutex::new(c)));
    }
    Ok(slot.clone())
}

/// Run `f` with the cached catalog and the MEPS id of `lang`; `None` when no
/// catalog is cached yet.
fn with_catalog<T>(
    app: &AppHandle,
    lang: &str,
    f: impl FnOnce(&Catalog, i64, &Library) -> ApiResult<T>,
) -> ApiResult<Option<T>> {
    let state = app.state::<AppState>();
    let _gate = state.cache_use()?;
    with_catalog_unguarded(app, lang, f)
}

/// [`with_catalog`] for callers that already hold the cache gate.
fn with_catalog_unguarded<T>(
    app: &AppHandle,
    lang: &str,
    f: impl FnOnce(&Catalog, i64, &Library) -> ApiResult<T>,
) -> ApiResult<Option<T>> {
    let state = app.state::<AppState>();
    let Some(catalog) = cached_catalog(app)? else {
        return Ok(None);
    };
    let catalog = catalog.lock().map_err(|e| e.to_string())?;
    let meps = catalog
        .meps_language(lang)
        .ok_or_else(|| format!("unknown language code {lang:?}"))?;
    let lib = state.lib()?;
    f(&catalog, meps, &lib).map(Some)
}

#[tauri::command(async)]
fn book_abbreviations(
    state: State<'_, AppState>,
    dir: String,
) -> ApiResult<std::collections::HashMap<i64, String>> {
    api::book_abbreviations(&*state.lib()?, &dir)
}

#[tauri::command(async)]
fn last_verse(state: State<'_, AppState>, dir: String, book: i64, chapter: i64) -> ApiResult<i64> {
    api::last_verse(&*state.lib()?, &dir, book, chapter)
}

#[tauri::command(async)]
fn research_verses(
    state: State<'_, AppState>,
    dir: String,
    book: i64,
    chapter: i64,
) -> ApiResult<Vec<i64>> {
    api::research_verses(&*state.lib()?, &dir, book, chapter)
}

#[tauri::command(async)]
fn research_guide(
    state: State<'_, AppState>,
    dir: String,
    book: i64,
    chapter: i64,
    verse: i64,
) -> ApiResult<Vec<api::ResearchEntry>> {
    api::research_guide(&*state.lib()?, &dir, book, chapter, verse)
}

#[tauri::command(async)]
fn chapter_study(
    state: State<'_, AppState>,
    dir: String,
    book: i64,
    chapter: i64,
) -> ApiResult<pergament::render::ChapterStudy> {
    api::chapter_study(&*state.lib()?, &dir, book, chapter)
}

#[tauri::command(async)]
fn book_overview(
    state: State<'_, AppState>,
    dir: String,
    book: i64,
) -> ApiResult<pergament::render::BookOverview> {
    api::book_overview(&*state.lib()?, &dir, book)
}

/// Whether a catalog is available without downloading.
#[tauri::command]
async fn catalog_cached(app: AppHandle) -> ApiResult<bool> {
    tauri::async_runtime::spawn_blocking(move || {
        let guard_state = app.state::<AppState>();
        let _gate = guard_state.cache_use()?;
        Ok(cached_catalog(&app)?.is_some())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Downloaded publications that have a newer version in the catalog. Looks at
/// the catalog only if one is cached already (this never downloads the first
/// one) and refreshes it when it is more than a day old; without a network the
/// cached catalog is used. `None` when there is no catalog yet.
#[tauri::command]
async fn check_updates(app: AppHandle) -> ApiResult<Option<Vec<api::UpdateInfo>>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _gate = state.cache_use()?;
        if Catalog::open_cached(&state.cache)
            .map_err(|e| e.to_string())?
            .is_none()
        {
            return Ok(None);
        }
        let client = Client::new(HttpConfig::default());
        let fresh = Catalog::load(
            &client,
            &state.cache,
            false,
            Duration::from_secs(24 * 3600),
            &mut progress(&app, "catalog"),
        );
        let catalog = match fresh {
            Ok(c) => c,
            // Offline or the server is down: the cached one is good enough.
            Err(_) => Catalog::open_cached(&state.cache)
                .map_err(|e| e.to_string())?
                .ok_or("no catalog")?,
        };
        let found =
            pergament::updates::available(&*state.lib()?, &catalog).map_err(|e| e.to_string())?;
        *state.catalog.lock().map_err(|e| e.to_string())? = Some(Arc::new(Mutex::new(catalog)));
        Ok(Some(found.into_iter().map(api::update_info).collect()))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Download (or refresh) the catalog; reports `progress` events.
#[tauri::command]
async fn load_catalog(app: AppHandle) -> ApiResult<()> {
    tauri::async_runtime::spawn_blocking(move || catalog(&app).map(|_| ()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn home_lists(
    app: AppHandle,
    lang: String,
    date: String,
) -> ApiResult<Option<api::HomeLists>> {
    tauri::async_runtime::spawn_blocking(move || {
        with_catalog(&app, &lang, |c, meps, lib| {
            api::home_lists(c, meps, lib, &date)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The most recently cataloged publications of the given languages, newest
/// first. `None` when no catalog is cached yet.
#[tauri::command]
async fn whats_new(app: AppHandle, langs: Vec<String>) -> ApiResult<Option<Vec<api::WhatsNewRow>>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _gate = state.cache_use()?;
        let Some(catalog) = cached_catalog(&app)? else {
            return Ok(None);
        };
        let catalog = catalog.lock().map_err(|e| e.to_string())?;
        let lib = state.lib()?;
        Ok(Some(api::whats_new(&catalog, &langs, &lib)?))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn teaching_toolbox(
    app: AppHandle,
    lang: String,
) -> ApiResult<Option<Vec<api::CatalogEntry>>> {
    tauri::async_runtime::spawn_blocking(move || {
        with_catalog(&app, &lang, |c, meps, lib| {
            api::teaching_toolbox(c, meps, lib)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn categories(app: AppHandle, lang: String) -> ApiResult<Option<Vec<api::Category>>> {
    tauri::async_runtime::spawn_blocking(move || {
        with_catalog(&app, &lang, |c, meps, _| api::categories(c, meps))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn category(
    app: AppHandle,
    lang: String,
    id: i64,
) -> ApiResult<Option<Vec<api::CatalogEntry>>> {
    tauri::async_runtime::spawn_blocking(move || {
        with_catalog(&app, &lang, |c, meps, lib| api::category(c, meps, lib, id))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn meetings(app: AppHandle, lang: String, date: String) -> ApiResult<Option<api::Meetings>> {
    tauri::async_runtime::spawn_blocking(move || {
        with_catalog(&app, &lang, |c, meps, lib| {
            api::meetings(c, meps, lib, &date)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn dated_page(
    app: AppHandle,
    lang: String,
    kind: String,
    date: String,
) -> ApiResult<Option<api::DatedPage>> {
    tauri::async_runtime::spawn_blocking(move || {
        api::dated_page(&*app.state::<AppState>().lib()?, &lang, &kind, &date)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn referenced_publications(
    app: AppHandle,
    dir: String,
    lang: String,
) -> ApiResult<Option<Vec<api::LanguageEntry>>> {
    tauri::async_runtime::spawn_blocking(move || {
        // The scan renders every document. It uses its own library handle so the
        // shared library and the catalog stay free for other commands meanwhile.
        let state = app.state::<AppState>();
        let own = Library::open(&state.root).map_err(|e| e.to_string())?;
        let hrefs = api::publication_hrefs(&own, &dir)?;
        // The cache is only needed for the lookup after the scan.
        let _gate = state.cache_use()?;
        let Some(catalog) = cached_catalog(&app)? else {
            return Ok(None);
        };
        let catalog = catalog.lock().map_err(|e| e.to_string())?;
        let meps = catalog
            .meps_language(&lang)
            .ok_or_else(|| format!("unknown language code {lang:?}"))?;
        api::missing_references(&catalog, meps, &own, &hrefs).map(Some)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn missing_entry(
    app: AppHandle,
    lang: String,
    href: String,
) -> ApiResult<Option<api::CatalogEntry>> {
    tauri::async_runtime::spawn_blocking(move || {
        with_catalog(&app, &lang, |c, meps, lib| {
            api::missing_entry(c, meps, lib, &href)
        })
        .map(Option::flatten)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn page_user_data(state: State<'_, AppState>, target: Target) -> ApiResult<api::PageUserData> {
    api::page_user_data(&*state.lib()?, &*state.user()?, &target)
}

/// Keep the text of an answer field of a page.
#[tauri::command]
fn save_answer(
    state: State<'_, AppState>,
    target: Target,
    tag: String,
    value: String,
) -> ApiResult<()> {
    let (loc, title) = api::page_location(&*state.lib()?, &target)?;
    let saved = state
        .user()?
        .save_answer(&loc, Some(&title), &tag, &value)
        .map_err(|e| e.to_string());
    edited(&state, saved)
}

/// Pass on a command's result; a success schedules the sync write.
/// Call it after the user data lock is released.
fn edited<T>(state: &AppState, result: ApiResult<T>) -> ApiResult<T> {
    if result.is_ok() {
        state.auto.changed();
    }
    result
}

#[tauri::command]
fn add_mark(
    state: State<'_, AppState>,
    target: Target,
    color: i64,
    ranges: Vec<Range>,
) -> ApiResult<String> {
    let (loc, title) = api::page_location(&*state.lib()?, &target)?;
    let added = state
        .user()?
        .add_mark(&loc, Some(&title), color, &ranges)
        .map_err(|e| e.to_string());
    edited(&state, added)
}

#[tauri::command]
fn set_mark_color(state: State<'_, AppState>, guid: String, color: i64) -> ApiResult<()> {
    let set = state
        .user()?
        .set_mark_color(&guid, color)
        .map_err(|e| e.to_string());
    edited(&state, set)
}

#[tauri::command]
fn delete_mark(state: State<'_, AppState>, guid: String) -> ApiResult<()> {
    let deleted = state.user()?.delete_mark(&guid).map_err(|e| e.to_string());
    edited(&state, deleted)
}

/// Create (with `target`) or update a note; returns its guid.
#[tauri::command]
fn save_note(
    state: State<'_, AppState>,
    target: Option<Target>,
    note: NoteInput,
) -> ApiResult<String> {
    let location = match &target {
        Some(t) => Some(api::page_location(&*state.lib()?, t)?),
        None => None,
    };
    let saved = state
        .user()?
        .save_note(
            location.as_ref().map(|(l, _)| l),
            location.as_ref().map(|(_, t)| t.as_str()),
            &note,
        )
        .map_err(|e| e.to_string());
    edited(&state, saved)
}

#[tauri::command]
fn delete_note(state: State<'_, AppState>, guid: String) -> ApiResult<()> {
    let deleted = state.user()?.delete_note(&guid).map_err(|e| e.to_string());
    edited(&state, deleted)
}

#[tauri::command]
fn all_notes(
    state: State<'_, AppState>,
    tag: Option<i64>,
) -> ApiResult<Vec<pergament::userdata::Note>> {
    state.user()?.all_notes(tag).map_err(|e| e.to_string())
}

#[tauri::command]
fn tags(state: State<'_, AppState>) -> ApiResult<Vec<pergament::userdata::TagInfo>> {
    state.user()?.tags().map_err(|e| e.to_string())
}

#[tauri::command]
fn favorites(state: State<'_, AppState>) -> ApiResult<Vec<pergament::userdata::PublicationKey>> {
    state.user()?.favorites().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_favorite(
    state: State<'_, AppState>,
    key: pergament::userdata::PublicationKey,
    favorite: bool,
) -> ApiResult<()> {
    let saved = state
        .user()?
        .set_favorite(&key, favorite)
        .map_err(|e| e.to_string());
    edited(&state, saved)
}

#[tauri::command]
fn bookmarks(state: State<'_, AppState>) -> ApiResult<Vec<pergament::userdata::Bookmark>> {
    state.user()?.bookmarks().map_err(|e| e.to_string())
}

#[tauri::command]
fn user_data_summary(state: State<'_, AppState>) -> ApiResult<pergament::userdata::Summary> {
    state.user()?.summary().map_err(|e| e.to_string())
}

#[tauri::command]
fn open_location(state: State<'_, AppState>, loc: Loc) -> ApiResult<Option<Target>> {
    api::open_location(&*state.lib()?, &loc)
}

#[tauri::command]
async fn export_backup(app: AppHandle, path: String) -> ApiResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _job = state.job();
        state
            .user()?
            .export_backup(std::path::Path::new(&path))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Replace all user data with a `.jwlibrary` backup. A rolling backup of
/// the current data is written first, as the safety copy does not outlive
/// the next sync merge. With sync on, the own file is written right away,
/// so the restore reaches the other computers.
#[tauri::command]
async fn restore_backup(app: AppHandle, path: String) -> ApiResult<pergament::userdata::Summary> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _job = state.job();
        let restored = {
            let mut user = state.user()?;
            state
                .auto
                .write_safety_backup(&user)
                .map_err(|e| format!("cannot back up the current data first: {e}"))?;
            user.restore_backup(std::path::Path::new(&path))
                .map_err(|e| e.to_string())
        };
        let restored = edited(&state, restored)?;
        state.auto.send(SyncMsg::WriteNow);
        Ok(restored)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Merge a `.jwlibrary` backup into the user data instead of replacing it.
#[tauri::command]
async fn merge_backup(app: AppHandle, path: String) -> ApiResult<pergament::userdata::MergeReport> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _job = state.job();
        let merged = state
            .user()?
            .merge_backup(std::path::Path::new(&path))
            .map_err(|e| e.to_string());
        edited(&state, merged)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Language list (code, names), cached for a week.
#[tauri::command]
async fn languages(app: AppHandle) -> ApiResult<Vec<Language>> {
    tauri::async_runtime::spawn_blocking(move || {
        let cache = app.state::<AppState>().cache.clone();
        let client = Client::new(HttpConfig::default());
        pergament::languages::load(&client, &cache, Duration::from_secs(7 * 24 * 3600))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn catalog_search(
    app: AppHandle,
    lang: String,
    query: String,
) -> ApiResult<Vec<api::CatalogEntry>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _gate = state.cache_use()?;
        catalog_unguarded(&app)?;
        with_catalog_unguarded(&app, &lang, |c, meps, lib| {
            api::search_entries(c, meps, lib, &query)
        })
        .map(Option::unwrap_or_default)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The publication in every catalog language; `None` when no catalog is cached.
#[tauri::command]
async fn catalog_languages(
    app: AppHandle,
    symbol: String,
    issue_tag: i64,
) -> ApiResult<Option<Vec<api::LanguageEntry>>> {
    tauri::async_runtime::spawn_blocking(move || {
        let guard_state = app.state::<AppState>();
        let _gate = guard_state.cache_use()?;
        let Some(catalog) = cached_catalog(&app)? else {
            return Ok(None);
        };
        let catalog = catalog.lock().map_err(|e| e.to_string())?;
        let state = app.state::<AppState>();
        let lib = state.lib()?;
        api::languages_of(&catalog, &lib, &symbol, issue_tag).map(Some)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A favorite's catalog tile; `None` when no catalog or entry is available.
#[tauri::command]
async fn favorite_entry(
    app: AppHandle,
    key: pergament::userdata::PublicationKey,
) -> ApiResult<Option<api::LanguageEntry>> {
    key.validate().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _gate = state.cache_use()?;
        let Some(catalog) = cached_catalog(&app)? else {
            return Ok(None);
        };
        let catalog = catalog.lock().map_err(|e| e.to_string())?;
        api::favorite_entry(&catalog, &*state.lib()?, &key)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn download_publication(
    app: AppHandle,
    item: CatalogItem,
    lang: String,
) -> ApiResult<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _job = state.job();
        let _gate = state.cache_use()?;
        let client = Client::new(HttpConfig::default());
        let mut lib = Library::open(&state.root).map_err(|e| e.to_string())?;
        let req = Request {
            key_symbol: &item.key_symbol,
            lang_code: &lang,
            issue_tag: Some(item.issue_tag),
        };
        let entry = remote::download(
            &client,
            &mut lib,
            &state.cache.join("downloads"),
            &req,
            Some(&item),
            &mut progress(
                &app,
                // The same key as `taskKey` in the UI.
                &format!(
                    "download:{}:{}:{}",
                    item.meps_language, item.symbol, item.issue_tag
                ),
            ),
        )
        .map_err(|e| e.to_string())?;
        Ok(entry.dir_name)
    })
    .await
    .map_err(|e| e.to_string())?
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
        let dir = app.state::<AppState>().cache.join("app-update");
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
                    &mut progress(&app, "app-update"),
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
fn running_jobs(state: State<'_, AppState>) -> usize {
    state.jobs.load(Ordering::SeqCst)
}

/// Start the installed (possibly just replaced) app again.
#[tauri::command]
fn restart_app(app: AppHandle) {
    // Goes through the exit events, so the close backup runs before the restart.
    app.request_restart();
}

/// Whether serving `path` requires a network fetch (an uncached remote image).
fn needs_fetch(app: &AppHandle, path: &str) -> bool {
    api::remote_image(&app.state::<AppState>().cache, path).is_some_and(|(_, f)| !f.is_file())
}

/// Catalog and media images are fetched once (rate limited) and cached; publication
/// images are read from the library.
fn media_response(app: &AppHandle, path: &str) -> Response<Vec<u8>> {
    let state = app.state::<AppState>();
    let file = match api::remote_image(&state.cache, path) {
        Some((url, cached)) => {
            if !cached.is_file() {
                let fetched = state.image_client.lock().ok().and_then(|client| {
                    let mut ignore = |_: u64, _: Option<u64>| {};
                    client
                        .download(&url, &cached, &Default::default(), &mut ignore)
                        .ok()
                });
                if fetched.is_none() {
                    let _ = std::fs::remove_file(pergament::net::part_path(&cached));
                }
            }
            cached.is_file().then_some(cached)
        }
        None => state
            .lib()
            .ok()
            .and_then(|lib| api::media_file(&lib, &state.root.join("publications"), path)),
    };
    match file.and_then(|f| std::fs::read(&f).ok().map(|bytes| (f, bytes))) {
        Some((f, bytes)) => Response::builder()
            .header("Content-Type", api::mime_for(&f))
            .header("Cache-Control", "max-age=3600")
            .body(bytes)
            .unwrap_or_default(),
        None => Response::builder()
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

/// Back up the data as the app closes. Runs at most once, whichever exit path gets here first.
fn backup_on_close(app: &AppHandle) {
    if app.state::<AppState>().auto.claim_close_backup() {
        backup_rolling(app, false);
        write_own_on_close(app);
    }
}

/// Write this computer's sync file as the app closes, if an edit has not
/// reached it yet. Stops the sync worker. Does nothing, and takes no lock,
/// while sync is off.
fn write_own_on_close(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.auto.sync_folder().is_none() {
        return;
    }
    let _job = state.job();
    // Taking the lock waits for a write or merge the worker is in the middle of.
    let user = match state.user() {
        Ok(user) => user,
        Err(e) => return eprintln!("sync write failed: {e}"),
    };
    let unwritten = state
        .auto
        .schedule
        .lock()
        .map_or(true, |mut s| s.on_close());
    let Some(folder) = state.auto.sync_folder().filter(|_| unwritten) else {
        return;
    };
    let mut sync_state = SyncState::load(&folder.state_dir);
    if let Err(e) = folder.write_own(&user, state.auto.host(), &mut sync_state) {
        eprintln!("sync write failed: {e}");
    }
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
        let state = app.state::<AppState>();
        let auto = &state.auto;
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
    let state = app.state::<AppState>();
    let auto = &state.auto;
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
    let state = app.state::<AppState>();
    let auto = &state.auto;
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
        let state = app.state::<AppState>();
        let path = path.map(PathBuf::from);
        if path.as_ref().is_some_and(|p| !p.is_dir()) {
            return Err("not a folder".to_owned());
        }
        let settings = {
            // No merge runs while the folder changes.
            let _job = state.job();
            let _user = state.user()?;
            state
                .auto
                .set_sync_folder(path)
                .map_err(|e| e.to_string())?
        };
        state.auto.send(SyncMsg::Reconfigure);
        if settings.sync.folder.is_some() {
            sync_and_wait(&state.auto, BTreeMap::new());
        }
        Ok(settings)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn sync_now(app: AppHandle) -> ApiResult<SyncStatus> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        sync_and_wait(&state.auto, BTreeMap::new());
        Ok(state.auto.sync_status())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn sync_status(state: State<'_, AppState>) -> SyncStatus {
    state.auto.sync_status()
}

/// Answer a held-back peer: `apply` its deletions, or keep the data here.
#[tauri::command]
async fn resolve_peer(app: AppHandle, file: String, apply: bool) -> ApiResult<SyncStatus> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = if apply {
            DeleteGuard::Apply
        } else {
            DeleteGuard::Skip
        };
        sync_and_wait(&state.auto, BTreeMap::from([(file, guard)]));
        Ok(state.auto.sync_status())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Write a rolling backup; with `only_if_due` only when a day has passed since the last one.
/// Errors are logged, never shown.
fn backup_rolling(app: &AppHandle, only_if_due: bool) {
    let state = app.state::<AppState>();
    if only_if_due {
        if !state.auto.rolling_due(auto::unix_now()) {
            return;
        }
    } else if !state.auto.settings().rolling.enabled {
        return;
    }
    let _job = state.job();
    let user = match state.user() {
        Ok(user) => user,
        Err(e) => return eprintln!("rolling backup failed: {e}"),
    };
    if let Err(e) = state.auto.write_rolling_now(&user) {
        eprintln!("rolling backup failed: {e}");
    }
}

#[tauri::command]
fn auto_settings(state: State<'_, AppState>) -> auto::AutoSettings {
    state.auto.settings()
}

#[tauri::command]
fn set_rolling(
    state: State<'_, AppState>,
    enabled: bool,
    keep: usize,
) -> ApiResult<auto::AutoSettings> {
    state
        .auto
        .set_rolling(enabled, keep)
        .map_err(|e| e.to_string())
}

/// Show the rolling backups folder in the file manager.
#[tauri::command]
fn open_backups_folder(state: State<'_, AppState>) -> ApiResult<()> {
    let dir = state.auto.rolling_dir();
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
    let library = Library::open(&root).expect("cannot open library");
    std::fs::create_dir_all(library.media_dir()).expect("cannot create media directory");
    let media_dir = library.media_dir();
    let user = UserData::open(root.join(userdata::DB_NAME)).expect("cannot open user data");
    let auto = auto::Auto::load(&root);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            library: Mutex::new(library),
            user: Mutex::new(user),
            root,
            cache,
            catalog: Mutex::new(None),
            cache_gate: RwLock::new(()),
            image_client: Mutex::new(Client::new(HttpConfig {
                min_interval: Duration::from_millis(100),
                max_retries: 2,
                ..HttpConfig::default()
            })),
            jobs: AtomicUsize::new(0),
            auto,
        })
        .on_window_event(|window, event| {
            let app = window.app_handle();
            if matches!(event, WindowEvent::Focused(true)) && window.label() == "main" {
                // Back at the app: bring in what the other computers did.
                app.state::<AppState>().auto.send(SyncMsg::Sync {
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
                    backup_on_close(app);
                }
                _ => {}
            }
        })
        .setup(move |app| {
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
                    backup_rolling(&handle, true);
                    std::thread::sleep(Duration::from_secs(3600));
                }
            });
            // Folder sync: one worker, starting with a round at startup.
            let (tx, rx) = mpsc::channel();
            let auto = &app.state::<AppState>().auto;
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
            favorites,
            set_favorite,
            favorite_entry,
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
                backup_on_close(app);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::JobGuard;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// The update waits for this count, so every way out of a job has to lower it.
    #[test]
    fn a_job_counts_until_it_ends() {
        let jobs = AtomicUsize::new(0);
        let failing = || -> Result<(), String> {
            let _job = JobGuard::new(&jobs);
            assert_eq!(jobs.load(Ordering::SeqCst), 1);
            Err("stopped early".to_owned())?;
            Ok(())
        };
        assert!(failing().is_err());
        assert_eq!(jobs.load(Ordering::SeqCst), 0);
        let a = JobGuard::new(&jobs);
        let b = JobGuard::new(&jobs);
        assert_eq!(jobs.load(Ordering::SeqCst), 2);
        drop((a, b));
        assert_eq!(jobs.load(Ordering::SeqCst), 0);
    }

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
