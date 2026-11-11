//! The app's state and the commands that need no window: the shells call these
//! and deliver their results to the page.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard};
use std::time::Duration;

use pergament::Library;
use pergament::catalog::{Catalog, CatalogItem};
use pergament::folder_sync::SyncState;
use pergament::languages::Language;
use pergament::links::MediaRef;
use pergament::mediator;
use pergament::navigate::{Page, Target};
use pergament::net::{Client, HttpConfig};
use pergament::remote::{self, Request};
use pergament::userdata::{self, Loc, NoteInput, Range, UserData};
use serde::Serialize;

use crate::api::{self, ApiResult, LinkAction, PubCard, PubDetail};
use crate::auto::{self, SyncMsg};

/// Where the library and the cache live. The shell decides; [`Core`] reads no environment.
pub struct Paths {
    pub library: PathBuf,
    pub cache: PathBuf,
}

/// What the shell around [`Core`] provides.
pub trait Host: Send + Sync + 'static {
    /// Deliver an app event (`progress`, `user-data-changed`, …) with a JSON payload.
    fn emit(&self, event: &str, payload: serde_json::Value);
}

/// How long a cached media category is used before it is fetched again.
const MEDIA_CATEGORY_TTL: Duration = Duration::from_secs(6 * 3600);

pub struct Core {
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
    host: Arc<dyn Host>,
}

/// Counts a running job in [`Core::running_jobs`] until it drops.
pub struct JobGuard<'a>(&'a AtomicUsize);

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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    task: String,
    done: u64,
    total: Option<u64>,
}

#[derive(Serialize)]
pub struct CacheUsage {
    #[serde(flatten)]
    usage: pergament::cache::Usage,
    total: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    path: String,
    title: Option<String>,
    error: Option<String>,
}

impl Core {
    /// Open the library and the user data in `paths.library`, creating the media folder.
    pub fn open(paths: Paths, host: Arc<dyn Host>) -> Result<Core, String> {
        let Paths {
            library: root,
            cache,
        } = paths;
        let library = Library::open(&root).map_err(|e| format!("cannot open library: {e}"))?;
        std::fs::create_dir_all(library.media_dir())
            .map_err(|e| format!("cannot create media directory: {e}"))?;
        let user = UserData::open(root.join(userdata::DB_NAME))
            .map_err(|e| format!("cannot open user data: {e}"))?;
        let auto = auto::Auto::load(&root);
        Ok(Core {
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
            host,
        })
    }

    // Accessors for the shell's own commands and workers.

    /// The library root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The cache directory.
    pub fn cache(&self) -> &Path {
        &self.cache
    }

    /// Where downloaded recordings are kept.
    pub fn media_dir(&self) -> ApiResult<PathBuf> {
        Ok(self.lib()?.media_dir())
    }

    /// Automatic backup and sync settings and state.
    pub fn auto(&self) -> &auto::Auto {
        &self.auto
    }

    /// The rate-limited client for catalog and media images.
    pub fn image_client(&self) -> &Mutex<Client> {
        &self.image_client
    }

    pub fn lib(&self) -> ApiResult<MutexGuard<'_, Library>> {
        self.library
            .lock()
            .map_err(|_| "library lock poisoned".to_owned())
    }

    pub fn user(&self) -> ApiResult<MutexGuard<'_, UserData>> {
        self.user
            .lock()
            .map_err(|_| "user data lock poisoned".to_owned())
    }

    /// Count a running job until the guard drops; an app update waits for it before restarting.
    pub fn job(&self) -> JobGuard<'_> {
        JobGuard::new(&self.jobs)
    }

    /// Use the cached catalog until the guard drops; clearing the cache waits for it.
    fn cache_use(&self) -> ApiResult<RwLockReadGuard<'_, ()>> {
        self.cache_gate
            .read()
            .map_err(|_| "cache lock poisoned".to_owned())
    }

    /// Report the progress of `task` as `progress` events, about once per percent.
    pub fn progress(&self, task: &str) -> impl FnMut(u64, Option<u64>) + use<> {
        let host = Arc::clone(&self.host);
        let task = task.to_owned();
        let mut last = u64::MAX;
        move |done, total| {
            // Emit roughly once per percent (or MiB without a total).
            let key = total.map_or(done >> 20, |t| done * 100 / t.max(1));
            if key != last {
                last = key;
                let progress = Progress {
                    task: task.clone(),
                    done,
                    total,
                };
                if let Ok(payload) = serde_json::to_value(progress) {
                    host.emit("progress", payload);
                }
            }
        }
    }

    // Cache.

    pub fn cache_usage(&self) -> ApiResult<CacheUsage> {
        let usage = pergament::cache::usage(&self.cache).map_err(|e| e.to_string())?;
        Ok(CacheUsage {
            total: usage.total(),
            usage,
        })
    }

    pub fn clear_cache(&self) -> ApiResult<()> {
        // PERGAMENT_CACHE may point anywhere; never empty the library with it.
        pergament::cache::check_safe(&self.cache, &self.root).map_err(|e| e.to_string())?;
        // Refuse while a download or catalog load is using the cache, instead of waiting: a
        // waiting writer would hold up every new catalog request behind it.
        let _gate = match self.cache_gate.try_write() {
            Ok(gate) => gate,
            Err(std::sync::TryLockError::WouldBlock) => {
                return Err("The cache is in use by a download or a catalog load. Try again when it has finished.".to_owned());
            }
            Err(_) => return Err("cache lock poisoned".to_owned()),
        };
        // Release the open catalog; it reloads on next use.
        *self.catalog.lock().map_err(|e| e.to_string())? = None;
        pergament::cache::clear(&self.cache).map_err(|e| e.to_string())
    }

    // Library and pages.

    pub fn list_publications(&self) -> ApiResult<Vec<PubCard>> {
        api::list_publications(&*self.lib()?)
    }

    pub fn publication(&self, dir: &str) -> ApiResult<PubDetail> {
        api::publication(&*self.lib()?, dir)
    }

    pub fn render_page(&self, target: &Target) -> ApiResult<Page> {
        api::render(&*self.lib()?, target)
    }

    pub fn link_action(&self, current: Option<&str>, href: &str) -> ApiResult<LinkAction> {
        api::link_action(&*self.lib()?, current, href)
    }

    /// The year text for the display's idle screen.
    pub fn year_text(
        &self,
        lang: &str,
        year: i64,
    ) -> ApiResult<Option<pergament::yeartext::YearText>> {
        api::year_text(&*self.lib()?, lang, year)
    }

    pub fn remove_publication(&self, dir: &str) -> ApiResult<()> {
        let mut lib = self.lib()?;
        let entry = lib
            .get_by_dir(dir)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("{dir} is not in the library"))?;
        lib.remove(&entry).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Import files with a library handle of their own.
    pub fn import_files(&self, paths: Vec<String>) -> ApiResult<Vec<ImportResult>> {
        let _job = self.job();
        let mut lib = Library::open(&self.root).map_err(|e| e.to_string())?;
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
    }

    pub fn book_abbreviations(
        &self,
        dir: &str,
    ) -> ApiResult<std::collections::HashMap<i64, String>> {
        api::book_abbreviations(&*self.lib()?, dir)
    }

    pub fn last_verse(&self, dir: &str, book: i64, chapter: i64) -> ApiResult<i64> {
        api::last_verse(&*self.lib()?, dir, book, chapter)
    }

    pub fn research_verses(&self, dir: &str, book: i64, chapter: i64) -> ApiResult<Vec<i64>> {
        api::research_verses(&*self.lib()?, dir, book, chapter)
    }

    pub fn research_guide(
        &self,
        dir: &str,
        book: i64,
        chapter: i64,
        verse: i64,
    ) -> ApiResult<Vec<api::ResearchEntry>> {
        api::research_guide(&*self.lib()?, dir, book, chapter, verse)
    }

    pub fn chapter_study(
        &self,
        dir: &str,
        book: i64,
        chapter: i64,
    ) -> ApiResult<pergament::render::ChapterStudy> {
        api::chapter_study(&*self.lib()?, dir, book, chapter)
    }

    pub fn book_overview(
        &self,
        dir: &str,
        book: i64,
    ) -> ApiResult<pergament::render::BookOverview> {
        api::book_overview(&*self.lib()?, dir, book)
    }

    pub fn dated_page(
        &self,
        lang: &str,
        kind: &str,
        date: &str,
    ) -> ApiResult<Option<api::DatedPage>> {
        api::dated_page(&*self.lib()?, lang, kind, date)
    }

    // Media.

    /// Renditions of a recording, smallest first. The player streams them.
    pub fn media_links(&self, media: &MediaRef) -> ApiResult<Vec<remote::MediaFile>> {
        let client = Client::new(HttpConfig::default());
        remote::media_links(&client, media).map_err(|e| e.to_string())
    }

    /// A video or audio category in a language, with thumbnails as `jwmedia:` URLs.
    /// `detailed` includes the recordings of subcategories.
    pub fn media_category(
        &self,
        lang: &str,
        key: &str,
        detailed: bool,
    ) -> ApiResult<mediator::Category> {
        let client = Client::new(HttpConfig::default());
        let mut category = mediator::category(
            &client,
            &self.cache,
            lang,
            key,
            detailed,
            MEDIA_CATEGORY_TTL,
        )
        .map_err(|e| e.to_string())?;
        category.map_images(&api::cms_image_url);
        Ok(category)
    }

    /// Download one quality of a recording from a category into the library.
    pub fn download_media(
        &self,
        lang: &str,
        category: &str,
        detailed: bool,
        key: &str,
        label: &str,
    ) -> ApiResult<api::MediaDownload> {
        let _job = self.job();
        let _gate = self.cache_use()?;
        let client = Client::new(HttpConfig::default());
        let found = mediator::category(
            &client,
            &self.cache,
            lang,
            category,
            detailed,
            MEDIA_CATEGORY_TTL,
        )
        .map_err(|e| e.to_string())?;
        let media = found
            .find(key)
            .ok_or_else(|| format!("{key} is not in {category}"))?;
        // A separate handle, so rendering is not blocked while the file downloads.
        let lib = Library::open(&self.root).map_err(|e| e.to_string())?;
        let entry = remote::download_media(
            &client,
            &lib,
            lang,
            media,
            label,
            &mut self.progress(&format!("media:{key}")),
        )
        .map_err(|e| e.to_string())?;
        Ok(api::media_download(&lib, entry))
    }

    pub fn media_downloads(&self) -> ApiResult<Vec<api::MediaDownload>> {
        let lib = self.lib()?;
        let list = lib.list_media().map_err(|e| e.to_string())?;
        Ok(list
            .into_iter()
            .map(|e| api::media_download(&lib, e))
            .collect())
    }

    pub fn remove_media(&self, id: i64) -> ApiResult<()> {
        self.lib()?.remove_media(id).map_err(|e| e.to_string())?;
        Ok(())
    }

    // Catalog.

    fn catalog(&self) -> ApiResult<Arc<Mutex<Catalog>>> {
        let _gate = self.cache_use()?;
        self.catalog_unguarded()
    }

    /// [`Core::catalog`] for callers that already hold the cache gate.
    fn catalog_unguarded(&self) -> ApiResult<Arc<Mutex<Catalog>>> {
        if let Some(c) = self.catalog.lock().map_err(|e| e.to_string())?.clone() {
            return Ok(c);
        }
        let _job = self.job();
        let client = Client::new(HttpConfig::default());
        let catalog = Catalog::load(
            &client,
            &self.cache,
            false,
            Duration::from_secs(24 * 3600),
            &mut self.progress("catalog"),
        )
        .map_err(|e| e.to_string())?;
        let catalog = Arc::new(Mutex::new(catalog));
        *self.catalog.lock().map_err(|e| e.to_string())? = Some(catalog.clone());
        Ok(catalog)
    }

    /// The catalog if it is loaded or cached; never downloads.
    fn cached_catalog(&self) -> ApiResult<Option<Arc<Mutex<Catalog>>>> {
        let mut slot = self.catalog.lock().map_err(|e| e.to_string())?;
        if slot.is_none()
            && let Some(c) = Catalog::open_cached(&self.cache).map_err(|e| e.to_string())?
        {
            *slot = Some(Arc::new(Mutex::new(c)));
        }
        Ok(slot.clone())
    }

    /// Run `f` with the cached catalog and the MEPS id of `lang`; `None` when no
    /// catalog is cached yet.
    fn with_catalog<T>(
        &self,
        lang: &str,
        f: impl FnOnce(&Catalog, i64, &Library) -> ApiResult<T>,
    ) -> ApiResult<Option<T>> {
        let _gate = self.cache_use()?;
        self.with_catalog_unguarded(lang, f)
    }

    /// [`Core::with_catalog`] for callers that already hold the cache gate.
    fn with_catalog_unguarded<T>(
        &self,
        lang: &str,
        f: impl FnOnce(&Catalog, i64, &Library) -> ApiResult<T>,
    ) -> ApiResult<Option<T>> {
        let Some(catalog) = self.cached_catalog()? else {
            return Ok(None);
        };
        let catalog = catalog.lock().map_err(|e| e.to_string())?;
        let meps = catalog
            .meps_language(lang)
            .ok_or_else(|| format!("unknown language code {lang:?}"))?;
        let lib = self.lib()?;
        f(&catalog, meps, &lib).map(Some)
    }

    /// Whether a catalog is available without downloading.
    pub fn catalog_cached(&self) -> ApiResult<bool> {
        let _gate = self.cache_use()?;
        Ok(self.cached_catalog()?.is_some())
    }

    /// Downloaded publications that have a newer version in the catalog. Looks at
    /// the catalog only if one is cached already (this never downloads the first
    /// one) and refreshes it when it is more than a day old; without a network the
    /// cached catalog is used. `None` when there is no catalog yet.
    pub fn check_updates(&self) -> ApiResult<Option<Vec<api::UpdateInfo>>> {
        let _gate = self.cache_use()?;
        if Catalog::open_cached(&self.cache)
            .map_err(|e| e.to_string())?
            .is_none()
        {
            return Ok(None);
        }
        let client = Client::new(HttpConfig::default());
        let fresh = Catalog::load(
            &client,
            &self.cache,
            false,
            Duration::from_secs(24 * 3600),
            &mut self.progress("catalog"),
        );
        let catalog = match fresh {
            Ok(c) => c,
            // Offline or the server is down: the cached one is good enough.
            Err(_) => Catalog::open_cached(&self.cache)
                .map_err(|e| e.to_string())?
                .ok_or("no catalog")?,
        };
        let found =
            pergament::updates::available(&*self.lib()?, &catalog).map_err(|e| e.to_string())?;
        *self.catalog.lock().map_err(|e| e.to_string())? = Some(Arc::new(Mutex::new(catalog)));
        Ok(Some(found.into_iter().map(api::update_info).collect()))
    }

    /// Download (or refresh) the catalog; reports `progress` events.
    pub fn load_catalog(&self) -> ApiResult<()> {
        self.catalog().map(|_| ())
    }

    pub fn home_lists(&self, lang: &str, date: &str) -> ApiResult<Option<api::HomeLists>> {
        self.with_catalog(lang, |c, meps, lib| api::home_lists(c, meps, lib, date))
    }

    /// The most recently cataloged publications of the given languages, newest
    /// first. `None` when no catalog is cached yet.
    pub fn whats_new(&self, langs: &[String]) -> ApiResult<Option<Vec<api::WhatsNewRow>>> {
        let _gate = self.cache_use()?;
        let Some(catalog) = self.cached_catalog()? else {
            return Ok(None);
        };
        let catalog = catalog.lock().map_err(|e| e.to_string())?;
        let lib = self.lib()?;
        Ok(Some(api::whats_new(&catalog, langs, &lib)?))
    }

    pub fn teaching_toolbox(&self, lang: &str) -> ApiResult<Option<Vec<api::CatalogEntry>>> {
        self.with_catalog(lang, api::teaching_toolbox)
    }

    pub fn categories(&self, lang: &str) -> ApiResult<Option<Vec<api::Category>>> {
        self.with_catalog(lang, |c, meps, _| api::categories(c, meps))
    }

    pub fn category(&self, lang: &str, id: i64) -> ApiResult<Option<Vec<api::CatalogEntry>>> {
        self.with_catalog(lang, |c, meps, lib| api::category(c, meps, lib, id))
    }

    pub fn meetings(&self, lang: &str, date: &str) -> ApiResult<Option<api::Meetings>> {
        self.with_catalog(lang, |c, meps, lib| api::meetings(c, meps, lib, date))
    }

    pub fn referenced_publications(
        &self,
        dir: &str,
        lang: &str,
    ) -> ApiResult<Option<Vec<api::LanguageEntry>>> {
        // The scan renders every document. It uses its own library handle so the
        // shared library and the catalog stay free for other commands meanwhile.
        let own = Library::open(&self.root).map_err(|e| e.to_string())?;
        let hrefs = api::publication_hrefs(&own, dir)?;
        // The cache is only needed for the lookup after the scan.
        let _gate = self.cache_use()?;
        let Some(catalog) = self.cached_catalog()? else {
            return Ok(None);
        };
        let catalog = catalog.lock().map_err(|e| e.to_string())?;
        let meps = catalog
            .meps_language(lang)
            .ok_or_else(|| format!("unknown language code {lang:?}"))?;
        api::missing_references(&catalog, meps, &own, &hrefs).map(Some)
    }

    pub fn missing_entry(&self, lang: &str, href: &str) -> ApiResult<Option<api::CatalogEntry>> {
        self.with_catalog(lang, |c, meps, lib| api::missing_entry(c, meps, lib, href))
            .map(Option::flatten)
    }

    /// Language list (code, names), cached for a week.
    pub fn languages(&self) -> ApiResult<Vec<Language>> {
        let client = Client::new(HttpConfig::default());
        pergament::languages::load(&client, &self.cache, Duration::from_secs(7 * 24 * 3600))
            .map_err(|e| e.to_string())
    }

    pub fn catalog_search(&self, lang: &str, query: &str) -> ApiResult<Vec<api::CatalogEntry>> {
        let _gate = self.cache_use()?;
        self.catalog_unguarded()?;
        self.with_catalog_unguarded(lang, |c, meps, lib| {
            api::search_entries(c, meps, lib, query)
        })
        .map(Option::unwrap_or_default)
    }

    /// The publication in every catalog language; `None` when no catalog is cached.
    pub fn catalog_languages(
        &self,
        symbol: &str,
        issue_tag: i64,
    ) -> ApiResult<Option<Vec<api::LanguageEntry>>> {
        let _gate = self.cache_use()?;
        let Some(catalog) = self.cached_catalog()? else {
            return Ok(None);
        };
        let catalog = catalog.lock().map_err(|e| e.to_string())?;
        let lib = self.lib()?;
        api::languages_of(&catalog, &lib, symbol, issue_tag).map(Some)
    }

    /// Catalog entries of favorite keys; `None` when no catalog is cached.
    pub fn favorite_entries(
        &self,
        lang: &str,
        keys: &[String],
    ) -> ApiResult<Option<Vec<api::CatalogEntry>>> {
        self.with_catalog(lang, |c, meps, lib| {
            api::favorite_entries(c, meps, lib, keys)
        })
    }

    pub fn download_publication(&self, item: &CatalogItem, lang: &str) -> ApiResult<String> {
        let _job = self.job();
        let _gate = self.cache_use()?;
        let client = Client::new(HttpConfig::default());
        let mut lib = Library::open(&self.root).map_err(|e| e.to_string())?;
        let req = Request {
            key_symbol: &item.key_symbol,
            lang_code: lang,
            issue_tag: Some(item.issue_tag),
        };
        let entry = remote::download(
            &client,
            &mut lib,
            &self.cache.join("downloads"),
            &req,
            Some(item),
            &mut self.progress(
                // The same key as `taskKey` in the UI.
                &format!(
                    "download:{}:{}:{}",
                    item.meps_language, item.symbol, item.issue_tag
                ),
            ),
        )
        .map_err(|e| e.to_string())?;
        Ok(entry.dir_name)
    }

    // User data.

    /// Pass on a command's result; a success schedules the sync write.
    /// Call it after the user data lock is released.
    fn edited<T>(&self, result: ApiResult<T>) -> ApiResult<T> {
        if result.is_ok() {
            self.auto.changed();
        }
        result
    }

    pub fn page_user_data(&self, target: &Target) -> ApiResult<api::PageUserData> {
        api::page_user_data(&*self.lib()?, &*self.user()?, target)
    }

    /// Keep the text of an answer field of a page.
    pub fn save_answer(&self, target: &Target, tag: &str, value: &str) -> ApiResult<()> {
        let (loc, title) = api::page_location(&*self.lib()?, target)?;
        let saved = self
            .user()?
            .save_answer(&loc, Some(&title), tag, value)
            .map_err(|e| e.to_string());
        self.edited(saved)
    }

    pub fn add_mark(&self, target: &Target, color: i64, ranges: &[Range]) -> ApiResult<String> {
        let (loc, title) = api::page_location(&*self.lib()?, target)?;
        let added = self
            .user()?
            .add_mark(&loc, Some(&title), color, ranges)
            .map_err(|e| e.to_string());
        self.edited(added)
    }

    pub fn set_mark_color(&self, guid: &str, color: i64) -> ApiResult<()> {
        let set = self
            .user()?
            .set_mark_color(guid, color)
            .map_err(|e| e.to_string());
        self.edited(set)
    }

    pub fn delete_mark(&self, guid: &str) -> ApiResult<()> {
        let deleted = self.user()?.delete_mark(guid).map_err(|e| e.to_string());
        self.edited(deleted)
    }

    /// Create (with `target`) or update a note; returns its guid.
    pub fn save_note(&self, target: Option<&Target>, note: &NoteInput) -> ApiResult<String> {
        let location = match target {
            Some(t) => Some(api::page_location(&*self.lib()?, t)?),
            None => None,
        };
        let saved = self
            .user()?
            .save_note(
                location.as_ref().map(|(l, _)| l),
                location.as_ref().map(|(_, t)| t.as_str()),
                note,
            )
            .map_err(|e| e.to_string());
        self.edited(saved)
    }

    pub fn delete_note(&self, guid: &str) -> ApiResult<()> {
        let deleted = self.user()?.delete_note(guid).map_err(|e| e.to_string());
        self.edited(deleted)
    }

    pub fn all_notes(&self, tag: Option<i64>) -> ApiResult<Vec<userdata::Note>> {
        self.user()?.all_notes(tag).map_err(|e| e.to_string())
    }

    pub fn tags(&self) -> ApiResult<Vec<userdata::TagInfo>> {
        self.user()?.tags().map_err(|e| e.to_string())
    }

    pub fn bookmarks(&self) -> ApiResult<Vec<userdata::Bookmark>> {
        self.user()?.bookmarks().map_err(|e| e.to_string())
    }

    pub fn user_data_summary(&self) -> ApiResult<userdata::Summary> {
        self.user()?.summary().map_err(|e| e.to_string())
    }

    pub fn open_location(&self, loc: &Loc) -> ApiResult<Option<Target>> {
        api::open_location(&*self.lib()?, loc)
    }

    pub fn export_backup(&self, path: &Path) -> ApiResult<()> {
        let _job = self.job();
        self.user()?.export_backup(path).map_err(|e| e.to_string())
    }

    /// Replace all user data with a `.jwlibrary` backup. A rolling backup of
    /// the current data is written first, as the safety copy does not outlive
    /// the next sync merge. With sync on, the own file is written right away,
    /// so the restore reaches the other computers.
    pub fn restore_backup(&self, path: &Path) -> ApiResult<userdata::Summary> {
        let _job = self.job();
        let restored = {
            let mut user = self.user()?;
            self.auto
                .write_safety_backup(&user)
                .map_err(|e| format!("cannot back up the current data first: {e}"))?;
            user.restore_backup(path).map_err(|e| e.to_string())
        };
        let restored = self.edited(restored)?;
        self.auto.send(SyncMsg::WriteNow);
        Ok(restored)
    }

    /// Merge a `.jwlibrary` backup into the user data instead of replacing it.
    pub fn merge_backup(&self, path: &Path) -> ApiResult<userdata::MergeReport> {
        let _job = self.job();
        let merged = self.user()?.merge_backup(path).map_err(|e| e.to_string());
        self.edited(merged)
    }

    /// Downloads and imports that are still running.
    pub fn running_jobs(&self) -> usize {
        self.jobs.load(Ordering::SeqCst)
    }

    // Backups on the way out.

    /// Back up the data as the app closes. Runs at most once, whichever exit path gets here first.
    pub fn backup_on_close(&self) {
        if self.auto.claim_close_backup() {
            self.backup_rolling(false);
            self.write_own_on_close();
        }
    }

    /// Write a rolling backup; with `only_if_due` only when a day has passed since the last one.
    /// Errors are logged, never shown.
    pub fn backup_rolling(&self, only_if_due: bool) {
        if only_if_due {
            if !self.auto.rolling_due(auto::unix_now()) {
                return;
            }
        } else if !self.auto.settings().rolling.enabled {
            return;
        }
        let _job = self.job();
        let user = match self.user() {
            Ok(user) => user,
            Err(e) => return eprintln!("rolling backup failed: {e}"),
        };
        if let Err(e) = self.auto.write_rolling_now(&user) {
            eprintln!("rolling backup failed: {e}");
        }
    }

    /// Write this computer's sync file as the app closes, if an edit has not
    /// reached it yet. Stops the sync worker. Does nothing, and takes no lock,
    /// while sync is off.
    fn write_own_on_close(&self) {
        if self.auto.sync_folder().is_none() {
            return;
        }
        let _job = self.job();
        // Taking the lock waits for a write or merge the worker is in the middle of.
        let user = match self.user() {
            Ok(user) => user,
            Err(e) => return eprintln!("sync write failed: {e}"),
        };
        let unwritten = self.auto.schedule.lock().map_or(true, |mut s| s.on_close());
        let Some(folder) = self.auto.sync_folder().filter(|_| unwritten) else {
            return;
        };
        let mut sync_state = SyncState::load(&folder.state_dir);
        if let Err(e) = folder.write_own(&user, self.auto.host(), &mut sync_state) {
            eprintln!("sync write failed: {e}");
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct NoHost;

    impl Host for NoHost {
        fn emit(&self, _event: &str, _payload: serde_json::Value) {}
    }

    /// A fresh library and cache open, and the library has no publications yet.
    #[test]
    fn opens_empty_library() {
        let library = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let paths = Paths {
            library: library.path().to_owned(),
            cache: cache.path().to_owned(),
        };
        let core = Core::open(paths, Arc::new(NoHost)).unwrap();
        assert_eq!(core.list_publications().unwrap().len(), 0);
    }

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
}
