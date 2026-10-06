//! Tauri shell: managed state, commands and the `jwmedia:` image protocol.
//! Logic lives in [`api`] and the `pergament` crate.

pub mod api;

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pergament::Library;
use pergament::catalog::{Catalog, CatalogItem};
use pergament::languages::Language;
use pergament::links::Link;
use pergament::navigate::{Page, Target};
use pergament::net::{Client, HttpConfig};
use pergament::remote::{self, Request};
use pergament::userdata::{self, Loc, NoteInput, Range, UserData};
use serde::Serialize;
use tauri::http::{Response, StatusCode};
use tauri::{AppHandle, Emitter, Manager, State};
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

/// Phones: the pickers there return URIs rather than plain paths.
const MOBILE: bool = cfg!(any(target_os = "android", target_os = "ios"));

/// A file chosen in a picker. Android returns `content://` URIs and iOS
/// security-scoped `file://` URLs, which `std::fs` cannot always open, so those
/// are copied into the cache first. The copy goes away on drop.
enum Picked {
    Path(PathBuf),
    Copy(tempfile::NamedTempFile),
}

impl Picked {
    fn path(&self) -> &Path {
        match self {
            Picked::Path(p) => p,
            Picked::Copy(f) => f.path(),
        }
    }
}

fn is_uri(location: &str) -> bool {
    MOBILE && location.contains("://")
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn open_uri(app: &AppHandle, uri: &str, write: bool) -> io::Result<File> {
    use std::str::FromStr;
    use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};

    let path = FilePath::from_str(uri).map_err(|e| io::Error::other(e.to_string()))?;
    let mut options = OpenOptions::new();
    if write {
        options.write(true).create(true).truncate(true);
    } else {
        options.read(true);
    }
    app.fs().open(path, options)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn open_uri(_app: &AppHandle, _uri: &str, _write: bool) -> io::Result<File> {
    Err(io::Error::other("URIs are only used on mobile"))
}

fn pick(app: &AppHandle, location: &str, cache: &Path) -> io::Result<Picked> {
    if !is_uri(location) {
        return Ok(Picked::Path(PathBuf::from(location)));
    }
    std::fs::create_dir_all(cache)?;
    let mut copy = tempfile::NamedTempFile::new_in(cache)?;
    io::copy(&mut open_uri(app, location, false)?, copy.as_file_mut())?;
    Ok(Picked::Copy(copy))
}

/// Import files on a worker thread with its own library handle.
#[tauri::command]
async fn import_files(app: AppHandle, paths: Vec<String>) -> ApiResult<Vec<ImportResult>> {
    let (root, cache) = {
        let state = app.state::<AppState>();
        (state.root.clone(), state.cache.clone())
    };
    tauri::async_runtime::spawn_blocking(move || {
        let mut lib = Library::open(&root).map_err(|e| e.to_string())?;
        Ok(paths
            .into_iter()
            .map(|path| {
                let imported = pick(&app, &path, &cache)
                    .map_err(|e| e.to_string())
                    .and_then(|file| lib.import(file.path()).map_err(|e| e.to_string()));
                match imported {
                    Ok(e) => ImportResult {
                        path,
                        title: Some(e.title),
                        error: None,
                    },
                    Err(error) => ImportResult {
                        path,
                        title: None,
                        error: Some(error),
                    },
                }
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
        let state = app.state::<AppState>();
        if !is_uri(&path) {
            return state
                .user()?
                .export_backup(Path::new(&path))
                .map_err(|e| e.to_string());
        }
        // The destination is a URI: write the backup to the cache, then copy it.
        std::fs::create_dir_all(&state.cache).map_err(|e| e.to_string())?;
        let staged = tempfile::NamedTempFile::new_in(&state.cache).map_err(|e| e.to_string())?;
        state
            .user()?
            .export_backup(staged.path())
            .map_err(|e| e.to_string())?;
        let mut src = File::open(staged.path()).map_err(|e| e.to_string())?;
        let mut dest = open_uri(&app, &path, true).map_err(|e| e.to_string())?;
        io::copy(&mut src, &mut dest).map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Replace all user data with a `.jwlibrary` backup.
#[tauri::command]
async fn restore_backup(app: AppHandle, path: String) -> ApiResult<pergament::userdata::Summary> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let file = pick(&app, &path, &state.cache).map_err(|e| e.to_string())?;
        state
            .user()?
            .restore_backup(file.path())
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

/// Whether serving `path` requires a network fetch (an uncached catalog image).
fn needs_fetch(app: &AppHandle, path: &str) -> bool {
    api::catalog_image_path(&app.state::<AppState>().cache, path).is_some_and(|(_, f)| !f.is_file())
}

/// Catalog images are fetched once (rate limited) and cached; publication
/// images are read from the library.
fn media_response(app: &AppHandle, path: &str) -> Response<Vec<u8>> {
    let state = app.state::<AppState>();
    let file = match api::catalog_image_path(&state.cache, path) {
        Some((rel, cached)) => {
            if !cached.is_file() {
                let fetched = state.image_client.lock().ok().and_then(|client| {
                    let url = format!("{}{rel}", pergament::catalog::IMAGE_BASE);
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

/// The library and cache folders. Desktop keeps the XDG locations; phones use
/// the app's private folders.
fn data_dirs(app: &tauri::App) -> Result<(PathBuf, PathBuf), Box<dyn std::error::Error>> {
    let root = match std::env::var_os("PERGAMENT_LIBRARY") {
        Some(dir) => PathBuf::from(dir),
        None if MOBILE => app.path().app_data_dir()?,
        None => dirs::data_dir()
            .map(|d| pergament::library::app_dir(&d))
            .ok_or("no data directory (set XDG_DATA_HOME or PERGAMENT_LIBRARY)")?,
    };
    let cache = match std::env::var_os("PERGAMENT_CACHE") {
        Some(dir) => PathBuf::from(dir),
        None if MOBILE => app.path().app_cache_dir()?,
        None => dirs::cache_dir()
            .map(|d| pergament::library::app_dir(&d))
            .ok_or("no cache directory (set XDG_CACHE_HOME or PERGAMENT_CACHE)")?,
    };
    Ok((root, cache))
}

#[cfg_attr(
    any(target_os = "android", target_os = "ios"),
    tauri::mobile_entry_point
)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init());
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let builder = builder.plugin(tauri_plugin_fs::init());

    builder
        .setup(|app| {
            let (root, cache) = data_dirs(app)?;
            let library = Library::open(&root)?;
            let user = UserData::open(root.join(userdata::DB_NAME))?;
            app.manage(AppState {
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
            });
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
            list_publications,
            publication,
            render_page,
            link_action,
            open_external,
            remove_publication,
            import_files,
            catalog_search,
            languages,
            chapter_study,
            catalog_cached,
            load_catalog,
            home_lists,
            categories,
            category,
            meetings,
            download_publication,
            dated_page,
            missing_entry,
            page_user_data,
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
