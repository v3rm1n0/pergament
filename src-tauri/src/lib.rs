//! Tauri shell: managed state, commands and the `jwmedia:` image protocol.
//! Logic lives in [`api`] and the `jwlinux` crate.

pub mod api;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jwlinux::Library;
use jwlinux::catalog::{Catalog, CatalogItem};
use jwlinux::languages::Language;
use jwlinux::links::Link;
use jwlinux::navigate::{Page, Target};
use jwlinux::net::{Client, HttpConfig};
use jwlinux::remote::{self, Request};
use serde::Serialize;
use tauri::http::{Response, StatusCode};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

use api::{ApiResult, LinkAction, PubCard, PubDetail};

pub struct AppState {
    library: Mutex<Library>,
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
fn chapter_study(
    state: State<'_, AppState>,
    dir: String,
    book: i64,
    chapter: i64,
) -> ApiResult<jwlinux::render::ChapterStudy> {
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

/// Language list (code, names), cached for a week.
#[tauri::command]
async fn languages(app: AppHandle) -> ApiResult<Vec<Language>> {
    tauri::async_runtime::spawn_blocking(move || {
        let cache = app.state::<AppState>().cache.clone();
        let client = Client::new(HttpConfig::default());
        jwlinux::languages::load(&client, &cache, Duration::from_secs(7 * 24 * 3600))
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
                    let url = format!("{}{rel}", jwlinux::catalog::IMAGE_BASE);
                    let mut ignore = |_: u64, _: Option<u64>| {};
                    client
                        .download(&url, &cached, &Default::default(), &mut ignore)
                        .ok()
                });
                if fetched.is_none() {
                    let _ = std::fs::remove_file(jwlinux::net::part_path(&cached));
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

pub fn run() {
    let root = std::env::var_os("JWL_LIBRARY")
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|d| d.join("jwlinux")))
        .expect("no data directory (set XDG_DATA_HOME or JWL_LIBRARY)");
    let cache = std::env::var_os("JWL_CACHE")
        .map(PathBuf::from)
        .or_else(|| dirs::cache_dir().map(|d| d.join("jwlinux")))
        .expect("no cache directory (set XDG_CACHE_HOME or JWL_CACHE)");
    let library = Library::open(&root).expect("cannot open library");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            library: Mutex::new(library),
            root,
            cache,
            catalog: Mutex::new(None),
            image_client: Mutex::new(Client::new(HttpConfig {
                min_interval: Duration::from_millis(100),
                max_retries: 2,
                ..HttpConfig::default()
            })),
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running jwlinux");
}
