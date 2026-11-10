//! Plain functions behind the Tauri commands, so they can be tested without
//! a running app. Everything returned is serialized for the frontend.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use pergament::catalog::{
    Catalog, CatalogItem, DATED_DAILY_TEXT, DATED_MEETING_WORKBOOK, DATED_WATCHTOWER_STUDY,
    LIST_MEETINGS, LIST_TEACHING_TOOLBOX, category_name, safe_image_path,
};
use pergament::library::MediaEntry;
use pergament::links::{Link, MediaKind, MediaRef};
use pergament::mediator;
use pergament::navigate::{self, Page, Target, TargetKind};
use pergament::reader::{BibleBook, TocNode};
use pergament::render::{RenderOptions, Renderer, media_name};
use pergament::userdata::{Loc, Mark, Note, PublicationKey, UserData};
use pergament::{Entry, Library, Publication};
use serde::Serialize;

pub type ApiResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// URL scheme serving publication images, see [`media_file`].
pub const MEDIA_SCHEME: &str = "jwmedia";

pub fn media_base(dir: &str) -> String {
    format!("{MEDIA_SCHEME}://localhost/{dir}/")
}

/// A publication as shown in the library.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PubCard {
    pub dir: String,
    pub title: String,
    pub short_title: Option<String>,
    pub symbol: String,
    pub key_symbol: String,
    pub year: i64,
    pub issue_tag: String,
    pub meps_language: i64,
    pub lang_code: Option<String>,
    pub publication_type: Option<String>,
    pub is_bible: bool,
    pub cover: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PubDetail {
    pub card: PubCard,
    pub toc: Vec<TocNode>,
    pub books: Vec<BibleBook>,
}

fn card(entry: &Entry, publication: &Publication) -> ApiResult<PubCard> {
    Ok(PubCard {
        dir: entry.dir_name.clone(),
        title: entry.title.clone(),
        short_title: entry.short_title.clone(),
        symbol: entry.symbol.clone(),
        key_symbol: publication.key_symbol().map_err(err)?,
        year: entry.year,
        issue_tag: entry.issue_tag.clone(),
        meps_language: entry.meps_language,
        lang_code: entry.lang_code.clone(),
        publication_type: entry.publication_type.clone(),
        is_bible: publication.is_bible(),
        cover: publication
            .cover_image()
            .map_err(err)?
            .map(|f| format!("{}{f}", media_base(&entry.dir_name))),
    })
}

fn open_entry(library: &Library, dir: &str) -> ApiResult<(Entry, Publication)> {
    let entry = library
        .get_by_dir(dir)
        .map_err(err)?
        .ok_or_else(|| format!("{dir} is not in the library"))?;
    let publication = Publication::open(library, &entry).map_err(err)?;
    Ok((entry, publication))
}

pub fn list_publications(library: &Library) -> ApiResult<Vec<PubCard>> {
    library
        .list()
        .map_err(err)?
        .iter()
        .map(|e| card(e, &Publication::open(library, e).map_err(err)?))
        .collect()
}

pub fn publication(library: &Library, dir: &str) -> ApiResult<PubDetail> {
    let (entry, publication) = open_entry(library, dir)?;
    Ok(PubDetail {
        card: card(&entry, &publication)?,
        toc: publication.toc().map_err(err)?,
        books: if publication.is_bible() {
            publication.bible_books().map_err(err)?
        } else {
            Vec::new()
        },
    })
}

/// Render a page as an HTML fragment with images served via [`MEDIA_SCHEME`].
pub fn render(library: &Library, target: &Target) -> ApiResult<Page> {
    navigate::render(library, target, false, |e, _| Some(media_base(&e.dir_name))).map_err(err)
}

pub fn chapter_study(
    library: &Library,
    dir: &str,
    book: i64,
    chapter: i64,
) -> ApiResult<pergament::render::ChapterStudy> {
    let (_, publication) = open_entry(library, dir)?;
    pergament::Renderer::new(
        &publication,
        pergament::RenderOptions {
            media_base: Some(media_base(dir)),
            standalone: false,
        },
    )
    .chapter_study(book, chapter)
    .map_err(err)
}

pub fn book_overview(
    library: &Library,
    dir: &str,
    book: i64,
) -> ApiResult<pergament::render::BookOverview> {
    let (entry, publication) = open_entry(library, dir)?;
    pergament::Renderer::new(
        &publication,
        pergament::RenderOptions {
            media_base: Some(media_base(dir)),
            standalone: false,
        },
    )
    .book_overview(book, entry.lang_code.as_deref())
    .map_err(err)
}

/// The Research Guide (`rsg`) in the language of a Bible of the library.
fn research_guide_publication(
    library: &Library,
    bible_dir: &str,
) -> ApiResult<Option<Publication>> {
    let (bible, _) = open_entry(library, bible_dir)?;
    let guides = library
        .find("rsg", Some(bible.meps_language), None)
        .map_err(err)?;
    guides
        .first()
        .map(|e| Publication::open(library, e).map_err(err))
        .transpose()
}

/// Verse numbers of a chapter the Research Guide has excerpts for; empty if
/// there is no Research Guide in the Bible's language.
pub fn research_verses(
    library: &Library,
    bible_dir: &str,
    book: i64,
    chapter: i64,
) -> ApiResult<Vec<i64>> {
    match research_guide_publication(library, bible_dir)? {
        Some(guide) => guide.guide_verses(book, chapter).map_err(err),
        None => Ok(Vec::new()),
    }
}

/// The highest verse number of a chapter of a Bible.
pub fn last_verse(library: &Library, bible_dir: &str, book: i64, chapter: i64) -> ApiResult<i64> {
    let (_, bible) = open_entry(library, bible_dir)?;
    bible.last_verse(book, chapter).map_err(err)
}

/// The abbreviations a Bible uses for its books in its own references ("1Ko", "Phil"), by book number.
/// Other publications have none.
pub fn book_abbreviations(library: &Library, bible_dir: &str) -> ApiResult<HashMap<i64, String>> {
    let (_, bible) = open_entry(library, bible_dir)?;
    if !bible.is_bible() {
        return Ok(HashMap::new());
    }
    bible.book_abbreviations().map_err(err)
}

/// An excerpt of the Research Guide for a verse.
#[derive(Debug, Clone, Serialize)]
pub struct ResearchEntry {
    /// e.g. "Insight".
    pub publication: String,
    /// e.g. "Creation".
    pub subject: String,
    /// e.g. `it-1 “Creation” par. 4`.
    pub location: String,
    /// `pergament://pub/…` link to the paragraphs in their publication.
    pub href: String,
    pub html: String,
}

pub fn research_guide(
    library: &Library,
    bible_dir: &str,
    book: i64,
    chapter: i64,
    verse: i64,
) -> ApiResult<Vec<ResearchEntry>> {
    let Some(guide) = research_guide_publication(library, bible_dir)? else {
        return Ok(Vec::new());
    };
    let renderer = pergament::Renderer::new(
        &guide,
        pergament::RenderOptions {
            media_base: None,
            standalone: false,
        },
    );
    guide
        .guide_extracts(book, chapter, verse)
        .map_err(err)?
        .into_iter()
        .map(|x| {
            Ok(ResearchEntry {
                publication: x.publication,
                subject: x.subject,
                location: x.location,
                href: format!(
                    "pergament://pub/{}",
                    x.link.strip_prefix("p/").unwrap_or(&x.link)
                ),
                html: renderer.fragment(&x.html).map_err(err)?,
            })
        })
        .collect()
}

/// A catalog publication, with the library directory if it is downloaded.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub item: CatalogItem,
    /// Category name of the publication type.
    pub category: Option<&'static str>,
    /// `jwmedia:` URL of the cover image (fetched and cached on demand).
    pub image_url: Option<String>,
    /// Library directory when the publication is already downloaded.
    pub local: Option<String>,
}

/// A downloaded publication with a newer version in the catalog.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// Library directory of the installed version.
    pub dir: String,
    /// Language code to download the update in.
    pub lang_code: String,
    /// Unpacked size of the installed version.
    pub installed_size: u64,
    /// The new version; `local` is the installed directory.
    pub entry: CatalogEntry,
}

pub fn update_info(update: pergament::updates::Update) -> UpdateInfo {
    UpdateInfo {
        entry: CatalogEntry {
            category: category_name(update.item.publication_type),
            image_url: update
                .item
                .image
                .as_ref()
                .map(|p| format!("{MEDIA_SCHEME}://localhost/{CATALOG_IMAGES}/{p}")),
            local: Some(update.dir.clone()),
            item: update.item,
        },
        dir: update.dir,
        lang_code: update.lang_code,
        installed_size: update.installed_size,
    }
}

/// The directory of each downloaded publication, by (symbol, issue tag, language).
type LocalMap = HashMap<(String, i64, i64), String>;

fn local_map(library: &Library) -> ApiResult<LocalMap> {
    Ok(library
        .list()
        .map_err(err)?
        .into_iter()
        .map(|e| {
            (
                (e.symbol, e.issue_tag.parse().unwrap_or(0), e.meps_language),
                e.dir_name,
            )
        })
        .collect())
}

fn entry_of(item: CatalogItem, local: &LocalMap) -> CatalogEntry {
    CatalogEntry {
        category: category_name(item.publication_type),
        image_url: item
            .image
            .as_ref()
            .map(|p| format!("{MEDIA_SCHEME}://localhost/{CATALOG_IMAGES}/{p}")),
        local: local
            .get(&(item.symbol.clone(), item.issue_tag, item.meps_language))
            .cloned(),
        item,
    }
}

fn entries(items: Vec<CatalogItem>, library: &Library) -> ApiResult<Vec<CatalogEntry>> {
    let local = local_map(library)?;
    Ok(items
        .into_iter()
        .map(|item| entry_of(item, &local))
        .collect())
}

pub fn search_entries(
    catalog: &Catalog,
    meps: i64,
    library: &Library,
    query: &str,
) -> ApiResult<Vec<CatalogEntry>> {
    entries(catalog.search(meps, query, 200).map_err(err)?, library)
}

/// A publication in one language, for the More Languages list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageEntry {
    /// Catalog language code, e.g. `X`.
    pub code: String,
    pub entry: CatalogEntry,
}

/// The publication in every language of the catalog.
pub fn languages_of(
    catalog: &Catalog,
    library: &Library,
    symbol: &str,
    issue_tag: i64,
) -> ApiResult<Vec<LanguageEntry>> {
    let items = catalog.languages_of(symbol, issue_tag).map_err(err)?;
    let codes: Vec<Option<String>> = items
        .iter()
        .map(|i| catalog.language_code(i.meps_language).map(str::to_owned))
        .collect();
    // The catalog lists some publications more than once; keep one row per language, the downloaded one if any.
    let mut out: Vec<LanguageEntry> = Vec::new();
    for (entry, code) in entries(items, library)?.into_iter().zip(codes) {
        let Some(code) = code else { continue };
        match out.iter_mut().find(|l| l.code == code) {
            Some(seen) if seen.entry.local.is_none() && entry.local.is_some() => seen.entry = entry,
            Some(_) => {}
            None => out.push(LanguageEntry { code, entry }),
        }
    }
    Ok(out)
}

/// Resolve one stored identity for display without changing it.
pub fn favorite_entry(
    catalog: &Catalog,
    library: &Library,
    key: &PublicationKey,
) -> ApiResult<Option<LanguageEntry>> {
    key.validate().map_err(err)?;
    let Some(item) = catalog
        .find(&key.key_symbol, key.meps_language, Some(key.issue_tag))
        .map_err(err)?
    else {
        return Ok(None);
    };
    let Some(code) = catalog.language_code(key.meps_language) else {
        return Ok(None);
    };
    Ok(Some(LanguageEntry {
        code: code.to_owned(),
        entry: entry_of(item, &local_map(library)?),
    }))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeLists {
    pub teaching_toolbox: Vec<CatalogEntry>,
    /// This year's daily text booklet.
    pub daily_text: Option<CatalogEntry>,
}

/// The curated Teaching Toolbox of one language, newest first.
pub fn teaching_toolbox(
    catalog: &Catalog,
    meps: i64,
    library: &Library,
) -> ApiResult<Vec<CatalogEntry>> {
    entries(
        catalog.curated(meps, LIST_TEACHING_TOOLBOX).map_err(err)?,
        library,
    )
}

pub fn home_lists(
    catalog: &Catalog,
    meps: i64,
    library: &Library,
    date: &str,
) -> ApiResult<HomeLists> {
    let daily = catalog
        .dated(meps, DATED_DAILY_TEXT, date)
        .map_err(err)?
        .into_iter()
        .map(|(item, _, _)| item)
        .next();
    Ok(HomeLists {
        teaching_toolbox: teaching_toolbox(catalog, meps, library)?,
        daily_text: entries(daily.into_iter().collect(), library)?
            .into_iter()
            .next(),
    })
}

/// A What's New row: a catalog entry with the language code it was found in.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhatsNewRow {
    pub lang_code: String,
    #[serde(flatten)]
    pub entry: CatalogEntry,
}

/// The most recently cataloged publications of several languages, newest
/// first; undated ones last. Codes the catalog does not know are errors.
pub fn whats_new(
    catalog: &Catalog,
    codes: &[String],
    library: &Library,
) -> ApiResult<Vec<WhatsNewRow>> {
    let mut rows: Vec<WhatsNewRow> = Vec::new();
    for code in codes {
        let meps = catalog
            .meps_language(code)
            .ok_or_else(|| format!("unknown language code {code:?}"))?;
        let items = catalog.whats_new(meps, 12).map_err(err)?;
        rows.extend(
            entries(items, library)?
                .into_iter()
                .map(|entry| WhatsNewRow {
                    lang_code: code.clone(),
                    entry,
                }),
        );
    }
    rows.sort_by(|a, b| b.entry.item.cataloged_on.cmp(&a.entry.item.cataloged_on));
    Ok(rows)
}

#[derive(Debug, Clone, Serialize)]
pub struct Category {
    pub id: i64,
    pub name: &'static str,
    pub count: i64,
}

pub fn categories(catalog: &Catalog, meps: i64) -> ApiResult<Vec<Category>> {
    Ok(catalog
        .categories(meps)
        .map_err(err)?
        .into_iter()
        .filter_map(|(id, count)| category_name(id).map(|name| Category { id, name, count }))
        .collect())
}

pub fn category(
    catalog: &Catalog,
    meps: i64,
    library: &Library,
    id: i64,
) -> ApiResult<Vec<CatalogEntry>> {
    entries(catalog.by_category(meps, id, 5000).map_err(err)?, library)
}

#[derive(Debug, Clone, Serialize)]
pub struct DatedEntry {
    pub entry: CatalogEntry,
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Meetings {
    pub workbook: Option<DatedEntry>,
    pub study: Option<DatedEntry>,
    pub other: Vec<CatalogEntry>,
}

pub fn meetings(
    catalog: &Catalog,
    meps: i64,
    library: &Library,
    date: &str,
) -> ApiResult<Meetings> {
    let dated = |class| -> ApiResult<Option<DatedEntry>> {
        let Some((item, start, end)) = catalog
            .dated(meps, class, date)
            .map_err(err)?
            .into_iter()
            .next()
        else {
            return Ok(None);
        };
        let entry = entries(vec![item], library)?.remove(0);
        Ok(Some(DatedEntry { entry, start, end }))
    };
    Ok(Meetings {
        workbook: dated(DATED_MEETING_WORKBOOK)?,
        study: dated(DATED_WATCHTOWER_STUDY)?,
        other: entries(catalog.curated(meps, LIST_MEETINGS).map_err(err)?, library)?,
    })
}

/// The material for one date from a downloaded dated publication.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatedPage {
    /// What to open: the day itself for a daily text, the week's program of a
    /// workbook, the study article of a study edition.
    pub target: Target,
    /// Title of the opened document (empty for a daily text).
    pub title: String,
    /// Dates covered, `YYYY-MM-DD`.
    pub start: String,
    pub end: String,
    /// The rendered dated entry (for a daily text: date, theme scripture, comment).
    pub html: String,
    /// First image of the opened document.
    pub image: Option<String>,
}

/// Key symbols of the dated publications, by the `kind` the frontend asks for.
fn dated_symbol(kind: &str) -> ApiResult<&'static str> {
    match kind {
        "dailyText" => Ok("es"),
        "workbook" => Ok("mwb"),
        "study" => Ok("w"),
        _ => Err(format!("unknown dated kind {kind:?}")),
    }
}

/// `2026-10-07` -> `20261007`.
fn date_number(date: &str) -> ApiResult<i64> {
    let digits: String = date.chars().filter(char::is_ascii_digit).collect();
    match (digits.len(), digits.parse()) {
        (8, Ok(n)) => Ok(n),
        _ => Err(format!("bad date {date:?}")),
    }
}

/// `20261007` -> `2026-10-07`.
fn date_string(n: i64) -> String {
    format!("{:04}-{:02}-{:02}", n / 10000, n / 100 % 100, n % 100)
}

/// Values of `attr="…"` in raw publication HTML, in order.
fn attr_values<'h>(html: &'h str, attr: &str) -> Vec<&'h str> {
    let needle = format!("{attr}=");
    html.match_indices(&needle)
        .filter_map(|(i, _)| {
            let rest = &html[i + needle.len()..];
            let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\''))?;
            let rest = &rest[1..];
            rest.find(quote).map(|end| &rest[..end])
        })
        .collect()
}

/// The document a dated entry stands for. A study edition's entry is a line of
/// its table of contents (class 68) that links to the week's article.
fn dated_document(
    publication: &Publication,
    dated: &pergament::reader::DatedText,
) -> ApiResult<i64> {
    let (info, _) = publication.document(dated.document_id).map_err(err)?;
    if info.class == "68" {
        for href in attr_values(&dated.content, "href") {
            let link = pergament::render::rewrite_href(href).and_then(|h| Link::parse(&h));
            if let Some(Link::Document {
                meps_document_id, ..
            }) = link
                && let Some(id) = publication
                    .document_by_meps_id(meps_document_id)
                    .map_err(err)?
            {
                return Ok(id);
            }
        }
    }
    Ok(dated.document_id)
}

/// The year text of `year` from the downloaded daily text booklet (`es26`
/// for 2026) in language `lang`.
pub fn year_text(
    library: &Library,
    lang: &str,
    year: i64,
) -> ApiResult<Option<pergament::yeartext::YearText>> {
    let symbol = format!("es{:02}", year.rem_euclid(100));
    let entry = library
        .list()
        .map_err(err)?
        .into_iter()
        .filter(|e| e.symbol == symbol && e.lang_code.as_deref() == Some(lang))
        .max_by_key(|e| e.imported_at);
    let Some(entry) = entry else {
        return Ok(None);
    };
    let publication = Publication::open(library, &entry).map_err(err)?;
    // It is on the title page, near the start.
    for info in publication.documents().map_err(err)?.into_iter().take(8) {
        if !info.has_content {
            continue;
        }
        let (_, content) = publication.document(info.id).map_err(err)?;
        if let Some(found) = content.as_deref().and_then(pergament::yeartext::find) {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

/// The daily text, workbook week or study article for `date` (`YYYY-MM-DD`)
/// from downloaded publications in language `lang`, newest issue first.
pub fn dated_page(
    library: &Library,
    lang: &str,
    kind: &str,
    date: &str,
) -> ApiResult<Option<DatedPage>> {
    let symbol = dated_symbol(kind)?;
    let day = date_number(date)?;
    let mut candidates: Vec<Entry> = library
        .list()
        .map_err(err)?
        .into_iter()
        .filter(|e| {
            e.lang_code.as_deref() == Some(lang)
                && e.symbol.trim_end_matches(|c: char| c.is_ascii_digit()) == symbol
        })
        .collect();
    candidates.sort_by(|a, b| (b.year, &b.issue_tag).cmp(&(a.year, &a.issue_tag)));
    for entry in candidates {
        let publication = Publication::open(library, &entry).map_err(err)?;
        let Some(dated) = publication.dated_text(day).map_err(err)? else {
            continue;
        };
        let base = media_base(&entry.dir_name);
        let html = pergament::Renderer::new(
            &publication,
            pergament::RenderOptions {
                media_base: Some(base.clone()),
                standalone: false,
            },
        )
        .dated_text(day)
        .map_err(err)?;
        let (kind, title, image) = if symbol == "es" {
            (TargetKind::Dated { date: day }, String::new(), None)
        } else {
            let id = dated_document(&publication, &dated)?;
            let (info, content) = publication.document(id).map_err(err)?;
            let image = content.as_deref().and_then(|c| {
                attr_values(c, "src")
                    .into_iter()
                    .find_map(media_name)
                    .map(|name| format!("{base}{name}"))
            });
            (TargetKind::Document(id), info.title, image)
        };
        return Ok(Some(DatedPage {
            target: Target {
                publication: entry.dir_name,
                kind,
            },
            title,
            start: date_string(dated.first),
            end: date_string(dated.last),
            html,
            image,
        }));
    }
    Ok(None)
}

/// The catalog publication a link points into. Bible links without a book
/// document fall back to the study Bible in `meps`.
fn linked_item(catalog: &Catalog, meps: i64, link: &Link) -> ApiResult<Option<CatalogItem>> {
    let in_language = |code: &str| catalog.meps_language(code).unwrap_or(meps);
    match link {
        Link::Document {
            lang_code,
            meps_document_id,
        } => catalog
            .by_document(in_language(lang_code), *meps_document_id)
            .map_err(err),
        Link::BookVerse {
            lang_code,
            book_document,
            ..
        } => catalog
            .by_document(in_language(lang_code), *book_document)
            .map_err(err),
        Link::Bible { .. } => catalog.find("nwtsty", meps, None).map_err(err),
        _ => Ok(None),
    }
}

/// The catalog publication a link points into, so it can be downloaded.
pub fn missing_entry(
    catalog: &Catalog,
    meps: i64,
    library: &Library,
    href: &str,
) -> ApiResult<Option<CatalogEntry>> {
    let Some(link) = Link::parse(href) else {
        return Ok(None);
    };
    let item = linked_item(catalog, meps, &link)?;
    Ok(entries(item.into_iter().collect(), library)?.pop())
}

/// `pergament://pub|verse|note/…` link targets in rendered HTML, first-seen order, deduped.
pub fn collect_hrefs(html: &str) -> Vec<String> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for value in attr_values(html, "href") {
        let wanted = [
            "pergament://pub/",
            "pergament://verse/",
            "pergament://note/",
        ]
        .iter()
        .any(|p| value.starts_with(p));
        if wanted && seen.insert(value) {
            out.push(value.to_owned());
        }
    }
    out
}

/// What a link points at, ignoring the position inside the document: links with
/// the same key resolve to the same publication.
#[derive(Hash, PartialEq, Eq)]
enum TargetKey {
    Document(String, i64),
    Bible,
}

fn target_key(link: &Link) -> Option<TargetKey> {
    match link {
        Link::Document {
            lang_code,
            meps_document_id,
        } => Some(TargetKey::Document(lang_code.clone(), *meps_document_id)),
        Link::BookVerse {
            lang_code,
            book_document,
            ..
        } => Some(TargetKey::Document(lang_code.clone(), *book_document)),
        Link::Bible { .. } => Some(TargetKey::Bible),
        _ => None,
    }
}

/// Catalog publications the links point into that cannot be opened yet: no
/// downloaded publication holds the target and the publication itself is not
/// downloaded. Each is listed once, with its catalog language code.
pub fn missing_references(
    catalog: &Catalog,
    meps: i64,
    library: &Library,
    hrefs: &[String],
) -> ApiResult<Vec<LanguageEntry>> {
    let local = local_map(library)?;
    let mut seen: HashSet<TargetKey> = HashSet::new();
    let mut found: HashSet<(String, i64, i64)> = HashSet::new();
    let mut out: Vec<LanguageEntry> = Vec::new();
    for href in hrefs {
        let Some(link) = Link::parse(href) else {
            continue;
        };
        let Some(key) = target_key(&link) else {
            continue;
        };
        if !seen.insert(key) {
            continue;
        }
        // Links that open already are not missing, whatever the catalog says.
        if navigate::resolve(library, None, &link)
            .map_err(err)?
            .is_some()
        {
            continue;
        }
        let Some(item) = linked_item(catalog, meps, &link)? else {
            continue;
        };
        let Some(code) = catalog.language_code(item.meps_language).map(str::to_owned) else {
            continue;
        };
        let entry = entry_of(item, &local);
        let id = (
            entry.item.symbol.clone(),
            entry.item.issue_tag,
            entry.item.meps_language,
        );
        if entry.local.is_none() && found.insert(id) {
            out.push(LanguageEntry { code, entry });
        }
    }
    Ok(out)
}

/// Every link target in the documents of `dir`; documents that fail to render are skipped.
pub fn publication_hrefs(library: &Library, dir: &str) -> ApiResult<Vec<String>> {
    let (_, publication) = open_entry(library, dir)?;
    let renderer = Renderer::new(&publication, RenderOptions::default());
    let mut seen: HashSet<String> = HashSet::new();
    let mut hrefs: Vec<String> = Vec::new();
    for doc in publication.documents().map_err(err)? {
        if !doc.has_content {
            continue;
        }
        let Ok(html) = renderer.document(doc.id) else {
            continue;
        };
        for h in collect_hrefs(&html) {
            if seen.insert(h.clone()) {
                hrefs.push(h);
            }
        }
    }
    Ok(hrefs)
}

/// The user data location of a page, and a title to store with it.
pub fn page_location(library: &Library, target: &Target) -> ApiResult<(Loc, String)> {
    let (entry, publication) = open_entry(library, &target.publication)?;
    let mut loc = Loc {
        key_symbol: publication.key_symbol().map_err(err)?,
        meps_language: entry.meps_language,
        issue_tag: entry.issue_tag.parse().unwrap_or(0),
        document_id: None,
        book: None,
        chapter: None,
    };
    let document = |id: i64| -> ApiResult<(i64, String)> {
        let (info, _) = publication.document(id).map_err(err)?;
        Ok((info.meps_document_id, info.title))
    };
    let title = match target.kind {
        TargetKind::Document(id) => {
            let (meps, title) = document(id)?;
            loc.document_id = Some(meps);
            title
        }
        TargetKind::Chapter { book, chapter, .. } => {
            loc.book = Some(book);
            loc.chapter = Some(chapter);
            format!(
                "{} {chapter}",
                publication.bible_book(book).map_err(err)?.chapter_title
            )
        }
        TargetKind::Dated { date } => {
            let dated = publication
                .dated_text(date)
                .map_err(err)?
                .ok_or_else(|| format!("no dated text for {date}"))?;
            let (meps, title) = document(dated.document_id)?;
            loc.document_id = Some(meps);
            title
        }
    };
    Ok((loc, title))
}

#[derive(Debug, Clone, Serialize)]
pub struct PageUserData {
    pub marks: Vec<Mark>,
    pub notes: Vec<Note>,
    /// Answer field texts by field id.
    pub answers: std::collections::BTreeMap<String, String>,
}

pub fn page_user_data(
    library: &Library,
    user: &UserData,
    target: &Target,
) -> ApiResult<PageUserData> {
    let (loc, _) = page_location(library, target)?;
    Ok(PageUserData {
        marks: user.marks(&loc).map_err(err)?,
        notes: user.notes(&loc).map_err(err)?,
        answers: user.answers(&loc).map_err(err)?,
    })
}

/// Where to open a note's or bookmark's page, if its publication is downloaded.
pub fn open_location(library: &Library, loc: &Loc) -> ApiResult<Option<Target>> {
    for entry in library.list().map_err(err)? {
        if entry.meps_language != loc.meps_language {
            continue;
        }
        let publication = Publication::open(library, &entry).map_err(err)?;
        if publication.key_symbol().map_err(err)? != loc.key_symbol
            || entry.issue_tag.parse::<i64>().unwrap_or(0) != loc.issue_tag
        {
            continue;
        }
        if let (Some(book), Some(chapter)) = (loc.book, loc.chapter)
            && publication.is_bible()
        {
            return Ok(Some(Target {
                publication: entry.dir_name,
                kind: TargetKind::Chapter {
                    book,
                    chapter,
                    verse: 1,
                },
            }));
        }
        if let Some(meps) = loc.document_id
            && let Some(id) = publication.document_by_meps_id(meps).map_err(err)?
        {
            return Ok(Some(Target {
                publication: entry.dir_name,
                kind: TargetKind::Document(id),
            }));
        }
    }
    Ok(None)
}

/// First path segment under which catalog images are served.
pub const CATALOG_IMAGES: &str = "catalog";

/// Cache path for a catalog image request path `/catalog/images/ab/x.jpg`.
pub fn catalog_image_path(cache: &Path, path: &str) -> Option<(String, PathBuf)> {
    let rel = path
        .trim_start_matches('/')
        .strip_prefix(CATALOG_IMAGES)?
        .strip_prefix('/')?;
    safe_image_path(rel).then(|| (rel.to_owned(), cache.join("catalog-images").join(rel)))
}

/// First path segment under which media catalog images are served.
pub const CMS_IMAGES: &str = "cms";

/// `jwmedia:` URL of a media catalog image (path below `mediator::IMAGE_BASE`).
pub fn cms_image_url(rel: &str) -> String {
    format!("{MEDIA_SCHEME}://localhost/{CMS_IMAGES}/{rel}")
}

/// Cache path for a media catalog image request path `/cms/p/x/y.jpg`.
pub fn cms_image_path(cache: &Path, path: &str) -> Option<(String, PathBuf)> {
    let rel = path
        .trim_start_matches('/')
        .strip_prefix(CMS_IMAGES)?
        .strip_prefix('/')?;
    mediator::safe_image_path(rel).then(|| (rel.to_owned(), cache.join("cms-images").join(rel)))
}

/// The remote image behind a request path: the URL to fetch and where it is cached.
pub fn remote_image(cache: &Path, path: &str) -> Option<(String, PathBuf)> {
    if let Some((rel, file)) = catalog_image_path(cache, path) {
        return Some((format!("{}{rel}", pergament::catalog::IMAGE_BASE), file));
    }
    let (rel, file) = cms_image_path(cache, path)?;
    Some((format!("{}{rel}", mediator::IMAGE_BASE), file))
}

/// A downloaded recording, with the paths the player needs.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaDownload {
    pub id: i64,
    pub key: String,
    pub lang_code: String,
    pub title: String,
    pub kind: MediaKind,
    pub label: String,
    pub size: i64,
    pub duration: Option<f64>,
    /// `jwmedia:` URL of the thumbnail.
    pub image: Option<String>,
    /// Absolute path of the video or audio file.
    pub path: String,
    /// Absolute path of the WebVTT captions.
    pub subtitle_path: Option<String>,
    pub downloaded_at: i64,
}

pub fn media_download(library: &Library, e: MediaEntry) -> MediaDownload {
    let dir = library.media_dir();
    let path = |f: &str| dir.join(f).to_string_lossy().into_owned();
    MediaDownload {
        id: e.id,
        path: path(&e.file_name),
        subtitle_path: e.subtitle_file.as_deref().map(path),
        image: e.image.as_deref().map(cms_image_url),
        key: e.key,
        lang_code: e.lang_code,
        title: e.title,
        kind: e.kind,
        label: e.label,
        size: e.size,
        duration: e.duration,
        downloaded_at: e.downloaded_at,
    }
}

/// What the frontend should do with a clicked link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LinkAction {
    /// `study_note`: show the study note of the target verse.
    Open {
        target: Target,
        #[serde(rename = "studyNote")]
        study_note: bool,
    },
    External {
        url: String,
    },
    /// A recording to play in the app.
    Media {
        media: MediaRef,
    },
    /// Not in the library; `url` opens it on jw.org.
    Missing {
        url: Option<String>,
    },
    /// Not in the library, but the current publication quotes it: show that
    /// embedded excerpt of the cited document.
    Extract {
        /// jw.org URL of the cited document, built exactly like `Missing`'s.
        url: Option<String>,
        publication: String,
        subject: String,
        location: String,
        /// Rewritten and sanitized excerpt HTML.
        html: String,
    },
    Ignore,
}

/// The `Extract.Link` form of a publication link href:
/// `pergament://pub/{lang}:{docid}[/{from}-{to}]` → `p/{lang}:{docid}[/{from}-{to}]`.
fn pub_link(href: &str) -> Option<String> {
    href.strip_prefix("pergament://pub/")
        .map(|r| format!("p/{r}"))
}

/// The excerpt an unresolvable citation link points at. `None` unless `link`
/// is a `Link::Document` inside an open publication and the publication quotes
/// the cited document; the html is rewritten and sanitized.
fn extract_fallback(
    library: &Library,
    current: Option<&Entry>,
    link: &Link,
    href: &str,
    url: Option<String>,
) -> ApiResult<Option<LinkAction>> {
    let Some(entry) = current else {
        return Ok(None);
    };
    if !matches!(link, Link::Document { .. }) {
        return Ok(None);
    }
    let Some(link) = pub_link(href) else {
        return Ok(None);
    };
    let publication = Publication::open(library, entry).map_err(err)?;
    let Some(x) = publication.citation_extract(&link).map_err(err)? else {
        return Ok(None);
    };
    let html = Renderer::new(
        &publication,
        RenderOptions {
            media_base: Some(media_base(&entry.dir_name)),
            standalone: false,
        },
    )
    .fragment(&x.html)
    .map_err(err)?;
    Ok(Some(LinkAction::Extract {
        url,
        publication: x.publication,
        subject: x.subject,
        location: x.location,
        html,
    }))
}

pub fn link_action(library: &Library, current: Option<&str>, href: &str) -> ApiResult<LinkAction> {
    let Some(link) = Link::parse(href) else {
        return Ok(LinkAction::Ignore);
    };
    let current = match current {
        Some(dir) => library.get_by_dir(dir).map_err(err)?,
        None => None,
    };
    Ok(match &link {
        Link::External(url) => LinkAction::External { url: url.clone() },
        Link::Media(media) => LinkAction::Media {
            media: media.clone(),
        },
        Link::Fragment(_) => LinkAction::Ignore,
        _ => match navigate::resolve(library, current.as_ref(), &link).map_err(err)? {
            Some(target) => LinkAction::Open {
                target,
                study_note: matches!(
                    link,
                    Link::BookVerse {
                        study_note: true,
                        ..
                    }
                ),
            },
            None => {
                let url = match &link {
                    Link::Document {
                        lang_code,
                        meps_document_id,
                    } => Some(format!(
                        "https://www.jw.org/finder?wtlocale={lang_code}&docid={meps_document_id}"
                    )),
                    _ => None,
                };
                // Only a citation of another publication reaches this: an
                // installed publication always wins via `navigate::resolve`.
                extract_fallback(library, current.as_ref(), &link, href, url.clone())?
                    .unwrap_or(LinkAction::Missing { url })
            }
        },
    })
}

/// Map a media request path `/{dir}/{file}` to a file in the library, or
/// `None` if it is not a plain image name inside an imported publication.
pub fn media_file(library: &Library, publications_root: &Path, path: &str) -> Option<PathBuf> {
    let mut parts = path.trim_start_matches('/').splitn(2, '/');
    let (dir, file) = (parts.next()?, parts.next()?);
    let src = format!("jwpub-media://{file}");
    let file = media_name(&src)?;
    let entry = library.get_by_dir(dir).ok()??;
    let full = publications_root.join(&entry.dir_name).join(file);
    full.is_file().then_some(full)
}

pub fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pergament::catalog::Catalog;

    /// A catalog with `w26` (August 2026) in German and English, `lff` in German only, a row listed twice
    /// (the real catalog has such duplicates) and one whose language has no cover to derive a code from.
    fn catalog(dir: &std::path::Path) -> Catalog {
        let path = dir.join("catalog-test.db");
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute_batch(
            "CREATE TABLE Publication (Id INTEGER PRIMARY KEY, MepsLanguageId INTEGER NOT NULL,
                 PublicationTypeId INTEGER, IssueTagNumber INTEGER NOT NULL, Title TEXT NOT NULL,
                 IssueTitle TEXT, ShortTitle TEXT NOT NULL, CoverTitle TEXT, Year INTEGER NOT NULL,
                 Symbol TEXT NOT NULL, KeySymbol TEXT);
             CREATE TABLE PublicationAsset (Id INTEGER PRIMARY KEY, PublicationId INTEGER NOT NULL,
                 MepsLanguageId INTEGER NOT NULL, Signature TEXT NOT NULL, Size INTEGER NOT NULL,
                 MimeType TEXT, CatalogedOn TEXT, ExpandedSize INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE ImageAsset (Id INTEGER PRIMARY KEY, NameFragment TEXT NOT NULL,
                 Width INTEGER NOT NULL DEFAULT 270, Height INTEGER NOT NULL DEFAULT 270);
             CREATE TABLE PublicationAssetImageMap (PublicationAssetId INTEGER, ImageAssetId INTEGER);
             CREATE TABLE PublicationAttribute (Name TEXT, Id INTEGER PRIMARY KEY);
             CREATE TABLE PublicationAttributeMap (PublicationId INTEGER, PublicationAttributeId INTEGER);
             CREATE TABLE CuratedAsset (ListType INTEGER, SortOrder INTEGER, PublicationAssetId INTEGER);
             CREATE TABLE DatedText (Class INTEGER, Start TEXT, End TEXT, PublicationId INTEGER);
             CREATE TABLE PublicationDocument (PublicationId INTEGER, DocumentId INTEGER);",
        )
        .unwrap();
        // (id, meps, issue, title, symbol, key symbol)
        let rows = [
            (1, 2, 20260800, "Wachtturm August 2026", "w26", "w"),
            (2, 0, 20260800, "Watchtower August 2026", "w26", "w"),
            (3, 2, 0, "Glücklich", "lff", "lff"),
            (4, 2, 0, "Glücklich (zweite Zeile)", "lff", "lff"),
            (5, 99, 0, "Without a derived language code", "zz", "zz"),
        ];
        for (id, meps, issue, title, symbol, key) in rows {
            c.execute(
                "INSERT INTO Publication VALUES (?1, ?2, 14, ?3, ?4, ?4, ?4, NULL, 2026, ?5, ?6)",
                rusqlite::params![id, meps, issue, title, symbol, key],
            )
            .unwrap();
            c.execute(
                "INSERT INTO PublicationAsset VALUES (?1, ?1, ?2, ?3, 1000, 'x', '2026-10-01T00:00:00+00:00', 2000)",
                rusqlite::params![id, meps, format!("{id:040}")],
            )
            .unwrap();
        }
        // Cover names are the evidence for a language's code.
        for (img, asset, name) in [
            (1, 1, "images/aa/1_X_cvr.jpg"),
            (2, 2, "images/bb/2_E_cvr.jpg"),
            (3, 3, "images/cc/3_X_cvr.jpg"),
        ] {
            c.execute(
                "INSERT INTO ImageAsset (Id, NameFragment) VALUES (?1, ?2)",
                rusqlite::params![img, name],
            )
            .unwrap();
            c.execute(
                "INSERT INTO PublicationAssetImageMap VALUES (?1, ?2)",
                rusqlite::params![asset, img],
            )
            .unwrap();
        }
        c.execute(
            "INSERT INTO PublicationDocument VALUES (1, 1001070144), (3, 1001070200)",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO CuratedAsset VALUES (?1, ?2, ?3), (?1, ?3, ?2)",
            rusqlite::params![LIST_TEACHING_TOOLBOX, 1, 3],
        )
        .unwrap();
        drop(c);
        Catalog::open(&path, "test".into()).unwrap()
    }

    fn keys(entries: &[CatalogEntry]) -> Vec<String> {
        entries
            .iter()
            .map(|e| {
                format!(
                    "{}_{}_{}",
                    e.item.symbol, e.item.meps_language, e.item.issue_tag
                )
            })
            .collect()
    }

    fn ref_keys(found: &[LanguageEntry]) -> Vec<String> {
        let entries: Vec<CatalogEntry> = found.iter().map(|l| l.entry.clone()).collect();
        keys(&entries)
    }

    #[test]
    fn collects_publication_hrefs_once() {
        let html = r##"<a href="pergament://pub/X:1001070144/1-1">a</a>
            <a href="https://example.org/">b</a> <a href="#fn1">c</a>
            <a href="pergament://pub/X:1001070144/1-1">dup</a>
            <a href='pergament://verse/X:1001070005/1:1'>v</a>"##;
        assert_eq!(
            collect_hrefs(html),
            vec![
                "pergament://pub/X:1001070144/1-1".to_owned(),
                "pergament://verse/X:1001070005/1:1".to_owned(),
            ]
        );
        assert!(collect_hrefs("<p>no links</p>").is_empty());
    }

    #[test]
    fn missing_references_lists_each_publication_once() {
        let dir = tempfile::tempdir().unwrap();
        let cat = catalog(dir.path());
        let lib = Library::open(dir.path().join("lib")).unwrap();
        let hrefs = vec![
            "pergament://pub/X:1001070144/1-1".to_owned(),
            "pergament://pub/X:1001070144/2-2".to_owned(), // same publication
            "pergament://pub/X:1001070200/1-1".to_owned(),
            "pergament://pub/X:999/1-1".to_owned(), // not in the catalog
        ];
        let found = missing_references(&cat, 2, &lib, &hrefs).unwrap();
        assert_eq!(ref_keys(&found), vec!["w26_2_20260800", "lff_2_0"]);
        assert!(found.iter().all(|e| e.entry.local.is_none()));
    }

    /// Pretend `symbol` (German, `issue`) is downloaded and holds the documents `meps_ids`.
    fn install(root: &std::path::Path, symbol: &str, issue: i64, meps_ids: &[i64]) {
        let dir = format!("{symbol}_2_{issue}");
        std::fs::create_dir_all(root.join("publications").join(&dir)).unwrap();
        let db =
            rusqlite::Connection::open(root.join("publications").join(&dir).join("c.db")).unwrap();
        db.execute_batch(
            "CREATE TABLE Document (DocumentId INTEGER PRIMARY KEY, MepsDocumentId INTEGER);",
        )
        .unwrap();
        for id in meps_ids {
            db.execute("INSERT INTO Document VALUES (?1, ?1)", [id])
                .unwrap();
        }
        drop(db);
        let index = rusqlite::Connection::open(root.join("index.sqlite")).unwrap();
        index
            .execute(
                "INSERT INTO publication (symbol, meps_language, issue_tag, year, title, dir_name,
                     db_file, contents_hash, imported_at)
                 VALUES (?1, 2, ?2, 2026, 't', ?3, 'c.db', 'h', 0)",
                rusqlite::params![symbol, issue.to_string(), dir],
            )
            .unwrap();
    }

    #[test]
    fn book_abbreviations_only_for_bibles() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("lib");
        let lib = Library::open(&root).unwrap();
        install(&root, "nwt", 0, &[]);
        install(&root, "w26", 20260800, &[]);
        let nwt = root.join("publications/nwt_2_0");
        rusqlite::Connection::open(nwt.join("c.db"))
            .unwrap()
            .execute_batch("CREATE TABLE BibleChapter (BookNumber INTEGER);")
            .unwrap();
        std::fs::write(
            nwt.join(".pergament-abbreviations.json"),
            r#"{"46":"1Ko","50":"Phil"}"#,
        )
        .unwrap();
        let abbr = book_abbreviations(&lib, "nwt_2_0").unwrap();
        assert_eq!(abbr.get(&46).map(String::as_str), Some("1Ko"));
        assert_eq!(abbr.get(&50).map(String::as_str), Some("Phil"));
        assert!(
            book_abbreviations(&lib, "w26_2_20260800")
                .unwrap()
                .is_empty()
        );
        assert!(book_abbreviations(&lib, "nope").is_err());
    }

    #[test]
    fn missing_references_carry_the_catalog_language_code() {
        let dir = tempfile::tempdir().unwrap();
        let cat = catalog(dir.path());
        let lib = Library::open(dir.path().join("lib")).unwrap();
        // The UI language is English (0), the link is German: the item and its code are German.
        let hrefs = vec!["pergament://pub/X:1001070144/1-1".to_owned()];
        let found = missing_references(&cat, 0, &lib, &hrefs).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, "X");
        assert_eq!(found[0].entry.item.meps_language, 2);
    }

    #[test]
    fn links_that_open_already_are_not_offered() {
        let dir = tempfile::tempdir().unwrap();
        let cat = catalog(dir.path());
        let root = dir.path().join("lib");
        let lib = Library::open(&root).unwrap();
        // An older issue holds the document the catalog attributes to the August issue.
        install(&root, "w25", 20250800, &[1001070144]);
        let hrefs = vec![
            "pergament://pub/X:1001070144/1-1".to_owned(),
            "pergament://pub/X:1001070200/1-1".to_owned(),
        ];
        let found = missing_references(&cat, 2, &lib, &hrefs).unwrap();
        assert_eq!(ref_keys(&found), vec!["lff_2_0"]);
    }

    #[test]
    fn downloaded_publications_are_not_offered() {
        let dir = tempfile::tempdir().unwrap();
        let cat = catalog(dir.path());
        let root = dir.path().join("lib");
        let lib = Library::open(&root).unwrap();
        // Downloaded, but the link target is not in it, so only the catalog entry says where it lives.
        install(&root, "w26", 20260800, &[42]);
        let hrefs = vec![
            "pergament://pub/X:1001070144/1-1".to_owned(),
            "pergament://pub/X:1001070200/1-1".to_owned(),
        ];
        let found = missing_references(&cat, 2, &lib, &hrefs).unwrap();
        assert_eq!(ref_keys(&found), vec!["lff_2_0"]);
    }

    #[test]
    fn lists_every_language_of_an_issue() {
        let tmp = tempfile::tempdir().unwrap();
        let (cat, lib) = (
            catalog(tmp.path()),
            Library::open(tmp.path().join("lib")).unwrap(),
        );
        let langs = languages_of(&cat, &lib, "w26", 20260800).unwrap();
        let codes: Vec<_> = langs.iter().map(|l| l.code.as_str()).collect();
        assert_eq!(codes, ["E", "X"]);
        assert!(langs.iter().all(|l| l.entry.local.is_none()));
    }

    #[test]
    fn languages_of_unknown_or_other_issues_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let (cat, lib) = (
            catalog(tmp.path()),
            Library::open(tmp.path().join("lib")).unwrap(),
        );
        assert!(
            languages_of(&cat, &lib, "w26", 20260900)
                .unwrap()
                .is_empty()
        );
        assert!(languages_of(&cat, &lib, "nope", 0).unwrap().is_empty());
        assert!(languages_of(&cat, &lib, "", 0).unwrap().is_empty());
        // A quote must not break the query.
        assert!(
            languages_of(&cat, &lib, "w26' OR '1'='1", 0)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn languages_without_a_code_are_left_out() {
        let tmp = tempfile::tempdir().unwrap();
        let (cat, lib) = (
            catalog(tmp.path()),
            Library::open(tmp.path().join("lib")).unwrap(),
        );
        assert!(languages_of(&cat, &lib, "zz", 0).unwrap().is_empty());
    }

    #[test]
    fn a_publication_listed_twice_is_one_language() {
        let tmp = tempfile::tempdir().unwrap();
        let (cat, lib) = (
            catalog(tmp.path()),
            Library::open(tmp.path().join("lib")).unwrap(),
        );
        let langs = languages_of(&cat, &lib, "lff", 0).unwrap();
        assert_eq!(
            langs.iter().map(|l| l.code.as_str()).collect::<Vec<_>>(),
            ["X"]
        );
    }

    #[test]
    fn whats_new_merges_languages_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let cat = catalog(dir.path());
        // Interleave dates across languages and leave one row undated.
        let conn = rusqlite::Connection::open(dir.path().join("catalog-test.db")).unwrap();
        conn.execute(
            "UPDATE PublicationAsset SET CatalogedOn = '2026-10-05T00:00:00+00:00' WHERE Id = 2",
            [],
        )
        .unwrap(); // E
        conn.execute(
            "UPDATE PublicationAsset SET CatalogedOn = '2026-10-04T00:00:00+00:00' WHERE Id = 3",
            [],
        )
        .unwrap(); // X
        conn.execute(
            "UPDATE PublicationAsset SET CatalogedOn = '2026-10-03T00:00:00+00:00' WHERE Id = 1",
            [],
        )
        .unwrap(); // X
        conn.execute(
            "UPDATE PublicationAsset SET CatalogedOn = NULL WHERE Id = 4",
            [],
        )
        .unwrap(); // X
        drop(conn);
        let lib = Library::open(dir.path().join("lib")).unwrap();
        let rows = whats_new(&cat, &["X".to_string(), "E".to_string()], &lib).unwrap();
        let keys: Vec<String> = rows
            .iter()
            .map(|r| {
                format!(
                    "{}_{}_{}",
                    r.entry.item.symbol, r.entry.item.meps_language, r.entry.item.issue_tag
                )
            })
            .collect();
        assert_eq!(
            keys,
            ["w26_0_20260800", "lff_2_0", "w26_2_20260800", "lff_2_0"]
        );
        let codes: Vec<&str> = rows.iter().map(|r| r.lang_code.as_str()).collect();
        assert_eq!(codes, ["E", "X", "X", "X"]);
    }

    #[test]
    fn whats_new_with_unknown_code_errors() {
        let dir = tempfile::tempdir().unwrap();
        let cat = catalog(dir.path());
        let lib = Library::open(dir.path().join("lib")).unwrap();
        let err = whats_new(&cat, &["X".to_string(), "ZZ".to_string()], &lib).unwrap_err();
        assert!(err.contains("unknown language code"));
    }

    #[test]
    fn whats_new_without_languages_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let cat = catalog(dir.path());
        let lib = Library::open(dir.path().join("lib")).unwrap();
        assert!(whats_new(&cat, &[], &lib).unwrap().is_empty());
    }

    #[test]
    fn favorite_tiles_resolve_stored_identities_in_their_own_language() {
        let tmp = tempfile::tempdir().unwrap();
        let (cat, lib) = (
            catalog(tmp.path()),
            Library::open(tmp.path().join("lib")).unwrap(),
        );
        install(lib.root(), "broken", 0, &[]);
        std::fs::write(
            lib.root().join("publications/broken_2_0/c.db"),
            b"unreadable database",
        )
        .unwrap();
        for (meps_language, code) in [(0, "E"), (2, "X")] {
            let key = PublicationKey {
                key_symbol: "w".into(),
                meps_language,
                issue_tag: 20260800,
            };
            let row = favorite_entry(&cat, &lib, &key).unwrap().unwrap();
            assert_eq!(row.code, code);
            assert_eq!(row.entry.item.key_symbol, key.key_symbol);
            assert_eq!(row.entry.item.symbol, "w26");
            assert_eq!(row.entry.item.meps_language, key.meps_language);
            assert_eq!(row.entry.item.issue_tag, key.issue_tag);
        }
        let missing = PublicationKey {
            key_symbol: "gone".into(),
            meps_language: 2,
            issue_tag: 0,
        };
        assert!(favorite_entry(&cat, &lib, &missing).unwrap().is_none());
        let invalid = PublicationKey {
            key_symbol: "".into(),
            ..missing
        };
        assert!(favorite_entry(&cat, &lib, &invalid).is_err());
        rusqlite::Connection::open(tmp.path().join("catalog-test.db"))
            .unwrap()
            .execute_batch("DROP TABLE Publication;")
            .unwrap();
        let key = PublicationKey {
            key_symbol: "w".into(),
            meps_language: 2,
            issue_tag: 20260800,
        };
        assert!(favorite_entry(&cat, &lib, &key).is_err());
    }

    #[test]
    fn teaching_toolbox_lists_only_the_curated_language() {
        let dir = tempfile::tempdir().unwrap();
        let cat = catalog(dir.path());
        let lib = Library::open(dir.path().join("lib")).unwrap();
        // German (meps 2) has lff and the German w26 in sort order; English (meps 0) has none listed.
        assert_eq!(
            keys(&teaching_toolbox(&cat, 2, &lib).unwrap()),
            vec!["lff_2_0", "w26_2_20260800"]
        );
        assert!(teaching_toolbox(&cat, 0, &lib).unwrap().is_empty());
    }

    #[test]
    fn link_actions_without_library_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = Library::open(tmp.path()).unwrap();
        assert_eq!(
            link_action(&lib, None, "https://www.jw.org/").unwrap(),
            LinkAction::External {
                url: "https://www.jw.org/".into()
            }
        );
        assert_eq!(
            link_action(&lib, None, "pergament://pub/X:2026003/").unwrap(),
            LinkAction::Missing {
                url: Some("https://www.jw.org/finder?wtlocale=X&docid=2026003".into())
            }
        );
        assert_eq!(
            link_action(&lib, None, "pergament://bible/19:23:1").unwrap(),
            LinkAction::Missing { url: None }
        );
        assert_eq!(
            link_action(&lib, None, "javascript:alert(1)").unwrap(),
            LinkAction::Ignore
        );
        assert_eq!(link_action(&lib, None, "#x").unwrap(), LinkAction::Ignore);
        assert!(matches!(
            link_action(
                &lib,
                None,
                "https://www.jw.org/finder?lank=pub-mwbv_202609_1_VIDEO&wtlocale=X"
            )
            .unwrap(),
            LinkAction::Media { .. }
        ));
    }

    #[test]
    fn extract_fallback_ignores_unknown_current() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = Library::open(tmp.path()).unwrap();
        assert_eq!(
            link_action(&lib, Some("tst_0"), "pergament://pub/X:2026003/").unwrap(),
            LinkAction::Missing {
                url: Some("https://www.jw.org/finder?wtlocale=X&docid=2026003".into())
            }
        );
    }

    #[test]
    fn pub_links_convert_to_extract_link_form() {
        assert_eq!(
            pub_link("pergament://pub/X:123/4-5").as_deref(),
            Some("p/X:123/4-5")
        );
        assert_eq!(
            pub_link("pergament://pub/X:123/").as_deref(),
            Some("p/X:123/")
        );
        assert_eq!(pub_link("pergament://bible/1:1:1"), None);
    }

    #[test]
    fn media_paths_are_confined() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = Library::open(tmp.path()).unwrap();
        let root = tmp.path().join("publications");
        // Nothing is imported, so even plausible paths are refused.
        for p in [
            "/nwtsty_2/a.jpg",
            "/../index.sqlite",
            "/nwtsty_2/../../index.sqlite",
            "/nwtsty_2/",
            "",
        ] {
            assert_eq!(media_file(&lib, &root, p), None, "{p}");
        }
    }

    #[test]
    fn catalog_image_paths() {
        let cache = Path::new("/c");
        assert_eq!(
            catalog_image_path(cache, "/catalog/images/ab/x_sqr-270x270.jpg"),
            Some((
                "images/ab/x_sqr-270x270.jpg".into(),
                PathBuf::from("/c/catalog-images/images/ab/x_sqr-270x270.jpg")
            ))
        );
        for bad in [
            "/catalog/images/../x.jpg",
            "/catalog/../../etc/passwd",
            "/other/images/a/b.jpg",
            "/catalog",
        ] {
            assert_eq!(catalog_image_path(cache, bad), None, "{bad}");
        }
    }

    #[test]
    fn cms_image_paths() {
        let cache = Path::new("/c");
        assert_eq!(
            remote_image(cache, "/cms/p/jwb-141/univ/art/x_wss_sm.jpg"),
            Some((
                "https://cms-imgp.jw-cdn.org/img/p/jwb-141/univ/art/x_wss_sm.jpg".into(),
                PathBuf::from("/c/cms-images/p/jwb-141/univ/art/x_wss_sm.jpg")
            ))
        );
        assert_eq!(
            remote_image(cache, "/catalog/images/ab/x.jpg").map(|(u, _)| u),
            Some("https://app.jw-cdn.org/catalogs/publications/images/ab/x.jpg".into())
        );
        assert_eq!(
            cms_image_url("p/a/b.jpg"),
            "jwmedia://localhost/cms/p/a/b.jpg"
        );
        for bad in [
            "/cms/p/../x.jpg",
            "/cms/x.jpg",
            "/cms/p/a/b.exe",
            "/cmsx/p/a/b.jpg",
            "/cms",
        ] {
            assert_eq!(remote_image(cache, bad), None, "{bad}");
        }
    }

    #[test]
    fn dates_and_attributes() {
        assert_eq!(date_number("2026-10-07").unwrap(), 20261007);
        assert!(date_number("2026-10").is_err());
        assert_eq!(date_string(20261007), "2026-10-07");
        assert!(dated_symbol("other").is_err());
        assert_eq!(
            attr_values(
                r##"<a href="jwpub://p/X:1/">a</a><img src="x.jpg"><a href="#f">"##,
                "href"
            ),
            ["jwpub://p/X:1/", "#f"]
        );
    }

    #[test]
    fn dated_pages_need_a_downloaded_publication() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = Library::open(tmp.path()).unwrap();
        assert!(
            dated_page(&lib, "X", "dailyText", "2026-10-07")
                .unwrap()
                .is_none()
        );
        assert!(dated_page(&lib, "X", "nope", "2026-10-07").is_err());
    }

    #[test]
    fn mime_types() {
        assert_eq!(mime_for(Path::new("a.JPG")), "image/jpeg");
        assert_eq!(mime_for(Path::new("a.png")), "image/png");
        assert_eq!(mime_for(Path::new("a")), "application/octet-stream");
    }
}
