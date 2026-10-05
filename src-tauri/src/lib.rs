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
) -> ApiResult<Vec<CatalogItem>> {
    tauri::async_runtime::spawn_blocking(move || {
        let catalog = catalog(&app)?;
        let catalog = catalog.lock().map_err(|e| e.to_string())?;
        let meps = catalog
            .meps_language(&lang)
            .ok_or_else(|| format!("unknown language code {lang:?}"))?;
        catalog.search(meps, &query, 200).map_err(|e| e.to_string())
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

fn media_response(app: &AppHandle, path: &str) -> Response<Vec<u8>> {
    let state = app.state::<AppState>();
    let file = state
        .lib()
        .ok()
        .and_then(|lib| api::media_file(&lib, &state.root.join("publications"), path));
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
        })
        .register_uri_scheme_protocol(api::MEDIA_SCHEME, |ctx, request| {
            media_response(ctx.app_handle(), request.uri().path())
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
            download_publication,
        ])
        .run(tauri::generate_context!())
        .expect("error while running jwlinux");
}
