// Typed wrappers around the Tauri commands in src-tauri/src/lib.rs.
import { invoke } from "@tauri-apps/api/core";

export interface TocNode {
  title: string;
  document_id: number | null;
  bible_book: number | null;
  children: TocNode[];
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

/** Mirrors jwlinux::navigate::Target (serde external tagging). */
export type TargetKind =
  | { document: number }
  | { chapter: { book: number; chapter: number; verse: number } };

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

export type LinkAction =
  | { kind: "open"; target: Target; studyNote: boolean }
  | { kind: "external"; url: string }
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
  removePublication: (dir: string) => invoke<void>("remove_publication", { dir }),
  importFiles: (paths: string[]) => invoke<ImportResult[]>("import_files", { paths }),
  languages: () => invoke<Language[]>("languages"),
  catalogSearch: (lang: string, query: string) =>
    invoke<CatalogItem[]>("catalog_search", { lang, query }),
  downloadPublication: (item: CatalogItem, lang: string) =>
    invoke<string>("download_publication", { item, lang }),
};

export function chapterTarget(publication: string, book: number, chapter: number, verse = 1): Target {
  return { publication, kind: { chapter: { book, chapter, verse } } };
}

export function documentTarget(publication: string, id: number): Target {
  return { publication, kind: { document: id } };
}
