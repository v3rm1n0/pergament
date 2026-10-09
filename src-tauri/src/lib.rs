//! Tauri shell: managed state, commands and the `jwmedia:` image protocol.
//! Logic lives in [`api`] and the `pergament` crate.

pub mod api;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pergament::Library;
use pergament::catalog::{Catalog, CatalogItem};
use pergament::languages::Language;
use pergament::links::{Link, MediaRef};
use pergament::mediator;
use pergament::navigate::{Page, Target};
use pergament::net::{Client, HttpConfig};
use pergament::remote::{self, Request};
use pergament::userdata::{self, Loc, NoteInput, Range, UserData};
use serde::Serialize;
use tauri::http::{Response, StatusCode};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_opener::OpenerExt;

use api::{ApiResult, LinkAction, PubCard, PubDetail};

pub struct AppState {
    library: Mutex<Library>,
    /// Highlights, notes and tags (`userData.db` in the library root).
    user: Mutex<UserData>,
    root: PathBuf,
    cache: PathBuf,
    catalog: Mutex<Option<Arc<Mutex<Catalog>>>>,
    /// Shared client for catalog images, so their requests are rate limited together.
    image_client: Mutex<Client>,
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

#[tauri::command]
fn media_downloads(state: State<'_, AppState>) -> ApiResult<Vec<api::MediaDownload>> {
    let lib = state.lib()?;
    let list = lib.list_media().map_err(|e| e.to_string())?;
    Ok(list
        .into_iter()
        .map(|e| api::media_download(&lib, e))
        .collect())
}

#[tauri::command]
fn remove_media(state: State<'_, AppState>, id: i64) -> ApiResult<()> {
    state.lib()?.remove_media(id).map_err(|e| e.to_string())?;
    Ok(())
}

/// Label of the second display window.
const DISPLAY: &str = "display";

/// The year text for the display's idle screen.
#[tauri::command]
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
#[tauri::command]
fn open_display(app: AppHandle) -> ApiResult<()> {
    if let Some(existing) = app.get_webview_window(DISPLAY) {
        return existing.show().map_err(|e| e.to_string());
    }
    let current = app
        .get_webview_window("main")
        .and_then(|w| w.current_monitor().ok().flatten());
    let other = app
        .available_monitors()
        .unwrap_or_default()
        .into_iter()
        .find(|m| {
            current
                .as_ref()
                .is_none_or(|c| c.position() != m.position() || c.size() != m.size())
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

/// Open an http(s) link in the browser. Anything else is refused.
#[tauri::command]
fn open_external(app: AppHandle, url: String) -> ApiResult<()> {
    match Link::parse(&url) {
        Some(Link::External(url)) => app
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|e| e.to_string()),
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
    if let Some(c) = state.catalog.lock().map_err(|e| e.to_string())?.clone() {
        return Ok(c);
    }
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
    let Some(catalog) = cached_catalog(app)? else {
        return Ok(None);
    };
    let catalog = catalog.lock().map_err(|e| e.to_string())?;
    let meps = catalog
        .meps_language(lang)
        .ok_or_else(|| format!("unknown language code {lang:?}"))?;
    let state = app.state::<AppState>();
    let lib = state.lib()?;
    f(&catalog, meps, &lib).map(Some)
}

#[tauri::command]
fn research_verses(
    state: State<'_, AppState>,
    dir: String,
    book: i64,
    chapter: i64,
) -> ApiResult<Vec<i64>> {
    api::research_verses(&*state.lib()?, &dir, book, chapter)
}

#[tauri::command]
fn research_guide(
    state: State<'_, AppState>,
    dir: String,
    book: i64,
    chapter: i64,
    verse: i64,
) -> ApiResult<Vec<api::ResearchEntry>> {
    api::research_guide(&*state.lib()?, &dir, book, chapter, verse)
}

#[tauri::command]
fn chapter_study(
    state: State<'_, AppState>,
    dir: String,
    book: i64,
    chapter: i64,
) -> ApiResult<pergament::render::ChapterStudy> {
    api::chapter_study(&*state.lib()?, &dir, book, chapter)
}

/// Whether a catalog is available without downloading.
#[tauri::command]
async fn catalog_cached(app: AppHandle) -> ApiResult<bool> {
    tauri::async_runtime::spawn_blocking(move || Ok(cached_catalog(&app)?.is_some()))
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
    state
        .user()?
        .save_answer(&loc, Some(&title), &tag, &value)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn add_mark(
    state: State<'_, AppState>,
    target: Target,
    color: i64,
    ranges: Vec<Range>,
) -> ApiResult<String> {
    let (loc, title) = api::page_location(&*state.lib()?, &target)?;
    state
        .user()?
        .add_mark(&loc, Some(&title), color, &ranges)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_mark_color(state: State<'_, AppState>, guid: String, color: i64) -> ApiResult<()> {
    state
        .user()?
        .set_mark_color(&guid, color)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_mark(state: State<'_, AppState>, guid: String) -> ApiResult<()> {
    state.user()?.delete_mark(&guid).map_err(|e| e.to_string())
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
    state
        .user()?
        .save_note(
            location.as_ref().map(|(l, _)| l),
            location.as_ref().map(|(_, t)| t.as_str()),
            &note,
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_note(state: State<'_, AppState>, guid: String) -> ApiResult<()> {
    state.user()?.delete_note(&guid).map_err(|e| e.to_string())
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
        app.state::<AppState>()
            .user()?
            .export_backup(std::path::Path::new(&path))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Replace all user data with a `.jwlibrary` backup.
#[tauri::command]
async fn restore_backup(app: AppHandle, path: String) -> ApiResult<pergament::userdata::Summary> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>()
            .user()?
            .restore_backup(std::path::Path::new(&path))
            .map_err(|e| e.to_string())
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
        catalog(&app)?;
        with_catalog(&app, &lang, |c, meps, lib| {
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

/// Catalog entries of favorite keys; `None` when no catalog is cached.
#[tauri::command]
async fn favorite_entries(
    app: AppHandle,
    lang: String,
    keys: Vec<String>,
) -> ApiResult<Option<Vec<api::CatalogEntry>>> {
    tauri::async_runtime::spawn_blocking(move || {
        with_catalog(&app, &lang, |c, meps, lib| {
            api::favorite_entries(c, meps, lib, &keys)
        })
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
                &format!("download:{}:{}", item.symbol, item.issue_tag),
            ),
        )
        .map_err(|e| e.to_string())?;
        Ok(entry.dir_name)
    })
    .await
    .map_err(|e| e.to_string())?
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

pub fn run() {
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

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            library: Mutex::new(library),
            user: Mutex::new(user),
            root,
            cache,
            catalog: Mutex::new(None),
            image_client: Mutex::new(Client::new(HttpConfig {
                min_interval: Duration::from_millis(100),
                max_retries: 2,
                ..HttpConfig::default()
            })),
        })
        .on_window_event(|window, event| {
            if !matches!(event, WindowEvent::Destroyed) {
                return;
            }
            let app = window.app_handle();
            match window.label() {
                // Tell the app the display was closed, so the setting follows.
                DISPLAY => {
                    let _ = app.emit_to("main", "display-closed", ());
                }
                // The display does not outlive the app window.
                "main" => {
                    if let Some(display) = app.get_webview_window(DISPLAY) {
                        let _ = display.close();
                    }
                }
                _ => {}
            }
        })
        .setup(move |app| {
            // Downloaded recordings are played through the asset protocol, which
            // supports range requests; only the media folder is reachable.
            app.asset_protocol_scope()
                .allow_directory(&media_dir, true)?;
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
            list_publications,
            publication,
            render_page,
            link_action,
            open_external,
            media_links,
            year_text,
            open_display,
            close_display,
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
            research_verses,
            research_guide,
            catalog_cached,
            load_catalog,
            check_updates,
            home_lists,
            categories,
            category,
            meetings,
            download_publication,
            dated_page,
            missing_entry,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running Pergament");
}
