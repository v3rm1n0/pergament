// Typed wrappers around the Tauri commands in src-tauri/src/lib.rs.
import { invoke } from "@tauri-apps/api/core";

export interface TocNode {
  title: string;
  document_id: number | null;
  bible_book: number | null;
  children: TocNode[];
}

/** Ids of the documents in a table of contents, in reading order. */
export function documentOrder(toc: TocNode[]): number[] {
  return toc.flatMap((n) => [...(n.document_id != null && n.children.length === 0 ? [n.document_id] : []), ...documentOrder(n.children)]);
}

export interface BibleBook {
  number: number;
  title: string;
  chapter_title: string;
  book_document_id: number | null;
  chapters: number;
}

export interface PubCard {
  dir: string;
  title: string;
  shortTitle: string | null;
  symbol: string;
  year: number;
  issueTag: string;
  mepsLanguage: number;
  langCode: string | null;
  publicationType: string | null;
  isBible: boolean;
  cover: string | null;
}

export interface PubDetail {
  card: PubCard;
  toc: TocNode[];
  books: BibleBook[];
}

/** Mirrors pergament::navigate::Target (serde external tagging). */
export type TargetKind =
  | { document: number }
  | { chapter: { book: number; chapter: number; verse: number } }
  | { dated: { date: number } };

export interface Target {
  publication: string;
  kind: TargetKind;
}

export interface Page {
  title: string;
  html: string;
  fragment: string | null;
  name: string;
}

/** A recording a publication links to (`pergament::links::MediaRef`). */
export interface MediaRef {
  pub_symbol: string;
  issue: string | null;
  track: number;
  kind: "audio" | "video";
  lang_code: string;
}

/** One playable rendition of a recording (`pergament::remote::MediaFile`). */
export interface MediaFile {
  title: string;
  label: string;
  mime: string;
  url: string;
  size: number | null;
  duration: number | null;
  poster: string | null;
  subtitles: string | null;
  md5: string | null;
}

/** A recording in the media catalog (`pergament::mediator::Media`). */
export interface Media {
  key: string;
  title: string;
  kind: "audio" | "video";
  duration: number | null;
  published: string | null;
  /** `jwmedia:` URL of the thumbnail. */
  image: string | null;
  files: MediaFile[];
}

/** A video or audio category (`pergament::mediator::Category`). */
export interface MediaCategory {
  key: string;
  name: string;
  container: boolean;
  image: string | null;
  subcategories: MediaCategory[];
  media: Media[];
}

/** A downloaded publication with a newer version in the catalog (`pergament_app::api::UpdateInfo`). */
export interface UpdateInfo {
  dir: string;
  langCode: string;
  installedSize: number;
  /** The new version; `local` is the installed directory. */
  entry: CatalogEntry;
}

/** The year's theme scripture (`pergament::yeartext::YearText`). */
export interface YearText {
  /** With the quotation marks of its language. */
  text: string;
  reference: string | null;
}

/** A recording saved in the library. */
export interface MediaDownload {
  id: number;
  key: string;
  langCode: string;
  title: string;
  kind: "audio" | "video";
  label: string;
  size: number;
  duration: number | null;
  image: string | null;
  path: string;
  subtitlePath: string | null;
  downloadedAt: number;
}

export type LinkAction =
  | { kind: "open"; target: Target; studyNote: boolean }
  | { kind: "external"; url: string }
  | { kind: "media"; media: MediaRef }
  | { kind: "missing"; url: string | null }
  | { kind: "ignore" };

export interface CatalogItem {
  key_symbol: string;
  symbol: string;
  meps_language: number;
  issue_tag: number;
  year: number;
  title: string;
  issue_title: string | null;
  size: number;
  sha1: string;
  publication_type: number;
  short_title: string | null;
  cataloged_on: string | null;
  image: string | null;
  /** Catalog attributes such as Archive, Yearbook or Study. */
  attributes: string[];
}

/** A catalog publication plus its library directory if downloaded. */
export interface CatalogEntry {
  item: CatalogItem;
  category: string | null;
  imageUrl: string | null;
  local: string | null;
}

export interface HomeLists {
  teachingToolbox: CatalogEntry[];
  whatsNew: CatalogEntry[];
  dailyText: CatalogEntry | null;
}

/** An excerpt the Research Guide lists for a verse. */
export interface ResearchEntry {
  publication: string;
  subject: string;
  location: string;
  href: string;
  html: string;
}

export interface Category {
  id: number;
  name: string;
  count: number;
}

export interface DatedEntry {
  entry: CatalogEntry;
  start: string;
  end: string;
}

export interface Meetings {
  workbook: DatedEntry | null;
  study: DatedEntry | null;
  other: CatalogEntry[];
}

export type DatedKind = "dailyText" | "workbook" | "study";

/** The material for one date from a downloaded dated publication. */
export interface DatedPage {
  target: Target;
  /** Title of the opened document (empty for a daily text). */
  title: string;
  start: string;
  end: string;
  html: string;
  image: string | null;
}

/** A page in user data (see pergament::userdata::Loc). */
export interface Loc {
  keySymbol: string;
  mepsLanguage: number;
  issueTag: number;
  documentId: number | null;
  book: number | null;
  chapter: number | null;
}

/** Highlighted tokens `start..=end` of a paragraph (1) or verse (2). */
export interface MarkRange {
  blockType: number;
  identifier: number;
  start: number;
  end: number;
}

export interface Mark {
  guid: string;
  /** 1-6, see `HIGHLIGHT_COLORS`. */
  color: number;
  ranges: MarkRange[];
}

export interface Note {
  guid: string;
  title: string;
  content: string;
  blockType: number;
  blockIdentifier: number | null;
  markGuid: string | null;
  color: number | null;
  tags: string[];
  lastModified: string;
  location: (Loc & { title: string | null }) | null;
}

export interface NoteInput {
  guid?: string;
  title: string;
  content: string;
  blockType?: number;
  blockIdentifier?: number | null;
  markGuid?: string | null;
  tags: string[];
}

export interface TagInfo {
  id: number;
  name: string;
  notes: number;
}

export interface Bookmark {
  slot: number;
  title: string;
  snippet: string | null;
  blockType: number;
  blockIdentifier: number | null;
  location: Loc & { title: string | null };
}

export interface UserDataSummary {
  marks: number;
  notes: number;
  tags: number;
  bookmarks: number;
  device: string | null;
}

export interface PageUserData {
  marks: Mark[];
  notes: Note[];
  /** Answer field texts by field id. */
  answers: Record<string, string>;
}

export interface OutlineEntry {
  level: number;
  text: string;
  begin_chapter: number;
  begin_verse: number;
  end_chapter: number;
  end_verse: number | null;
}

export interface VerseStudy {
  chapter: number;
  verse: number;
  footnotes: { marker: string; index: number; html: string }[];
  xrefs: { marker: string; block: number; refs: { href: string; label: string }[] }[];
  notes: string[];
}

export interface ChapterStudy {
  outlineTitle: string | null;
  outline: OutlineEntry[];
  verses: VerseStudy[];
}

export interface Language {
  code: string;
  name: string;
  vernacular: string;
  rtl: boolean;
  signLanguage: boolean;
}

export interface ImportResult {
  path: string;
  title: string | null;
  error: string | null;
}

export interface Progress {
  task: string;
  done: number;
  total: number | null;
}

export const api = {
  listPublications: () => invoke<PubCard[]>("list_publications"),
  publication: (dir: string) => invoke<PubDetail>("publication", { dir }),
  renderPage: (target: Target) => invoke<Page>("render_page", { target }),
  linkAction: (current: string | null, href: string) =>
    invoke<LinkAction>("link_action", { current, href }),
  openExternal: (url: string) => invoke<void>("open_external", { url }),
  mediaLinks: (media: MediaRef) => invoke<MediaFile[]>("media_links", { media }),
  mediaCategory: (lang: string, key: string, detailed: boolean) =>
    invoke<MediaCategory>("media_category", { lang, key, detailed }),
  downloadMedia: (lang: string, category: string, detailed: boolean, key: string, label: string) =>
    invoke<MediaDownload>("download_media", { lang, category, detailed, key, label }),
  mediaDownloads: () => invoke<MediaDownload[]>("media_downloads"),
  removeMedia: (id: number) => invoke<void>("remove_media", { id }),
  /** `null` when no catalog is cached yet. */
  checkUpdates: () => invoke<UpdateInfo[] | null>("check_updates"),
  yearText: (lang: string, year: number) => invoke<YearText | null>("year_text", { lang, year }),
  openDisplay: () => invoke<void>("open_display"),
  closeDisplay: () => invoke<void>("close_display"),
  removePublication: (dir: string) => invoke<void>("remove_publication", { dir }),
  importFiles: (paths: string[]) => invoke<ImportResult[]>("import_files", { paths }),
  languages: () => invoke<Language[]>("languages"),
  catalogSearch: (lang: string, query: string) =>
    invoke<CatalogEntry[]>("catalog_search", { lang, query }),
  chapterStudy: (dir: string, book: number, chapter: number) =>
    invoke<ChapterStudy>("chapter_study", { dir, book, chapter }),
  catalogCached: () => invoke<boolean>("catalog_cached"),
  loadCatalog: () => invoke<void>("load_catalog"),
  homeLists: (lang: string, date: string) => invoke<HomeLists | null>("home_lists", { lang, date }),
  categories: (lang: string) => invoke<Category[] | null>("categories", { lang }),
  category: (lang: string, id: number) => invoke<CatalogEntry[] | null>("category", { lang, id }),
  meetings: (lang: string, date: string) => invoke<Meetings | null>("meetings", { lang, date }),
  downloadPublication: (item: CatalogItem, lang: string) =>
    invoke<string>("download_publication", { item, lang }),
  datedPage: (lang: string, kind: DatedKind, date: string) =>
    invoke<DatedPage | null>("dated_page", { lang, kind, date }),
  missingEntry: (lang: string, href: string) => invoke<CatalogEntry | null>("missing_entry", { lang, href }),
  pageUserData: (target: Target) => invoke<PageUserData>("page_user_data", { target }),
  saveAnswer: (target: Target, tag: string, value: string) => invoke<void>("save_answer", { target, tag, value }),
  addMark: (target: Target, color: number, ranges: MarkRange[]) =>
    invoke<string>("add_mark", { target, color, ranges }),
  setMarkColor: (guid: string, color: number) => invoke<void>("set_mark_color", { guid, color }),
  deleteMark: (guid: string) => invoke<void>("delete_mark", { guid }),
  saveNote: (target: Target | null, note: NoteInput) => invoke<string>("save_note", { target, note }),
  deleteNote: (guid: string) => invoke<void>("delete_note", { guid }),
  allNotes: (tag: number | null) => invoke<Note[]>("all_notes", { tag }),
  tags: () => invoke<TagInfo[]>("tags"),
  bookmarks: () => invoke<Bookmark[]>("bookmarks"),
  userDataSummary: () => invoke<UserDataSummary>("user_data_summary"),
  openLocation: (loc: Loc) => invoke<Target | null>("open_location", { loc }),
  exportBackup: (path: string) => invoke<void>("export_backup", { path }),
  restoreBackup: (path: string) => invoke<UserDataSummary>("restore_backup", { path }),
  media: (url: string) => invoke<ArrayBuffer>("media", { url }),
  researchVerses: (dir: string, book: number, chapter: number) =>
    invoke<number[]>("research_verses", { dir, book, chapter }),
  researchGuide: (dir: string, book: number, chapter: number, verse: number) =>
    invoke<ResearchEntry[]>("research_guide", { dir, book, chapter, verse }),
};

export function chapterTarget(publication: string, book: number, chapter: number, verse = 1): Target {
  return { publication, kind: { chapter: { book, chapter, verse } } };
}

export function documentTarget(publication: string, id: number): Target {
  return { publication, kind: { document: id } };
}
