//! Name-based dispatch of the portable commands, for shells without Tauri's command layer.
//!
//! The argument keys are the ones the web UI sends (`ui/src/lib/api.ts`): the JS object arrives
//! unchanged, so multi-word keys are camelCase.

use std::path::Path;

use pergament::catalog::CatalogItem;
use pergament::links::MediaRef;
use pergament::navigate::Target;
use pergament::userdata::{Loc, NoteInput, Range};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::api::ApiResult;
use crate::core::Core;

/// Builders for synthetic `.jwpub` files, shared with the root crate's integration tests.
#[cfg(test)]
#[path = "../../tests/common/mod.rs"]
mod common;

/// Commands without arguments that go to the network when they run.
#[cfg(test)]
const NETWORK: &[&str] = &["check_updates", "load_catalog", "languages"];

/// Every command [`Core::invoke`] dispatches.
#[cfg(test)]
const COMMANDS: &[&str] = &[
    "cache_usage",
    "clear_cache",
    "list_publications",
    "publication",
    "render_page",
    "link_action",
    "year_text",
    "remove_publication",
    "import_files",
    "book_abbreviations",
    "last_verse",
    "research_verses",
    "research_guide",
    "chapter_study",
    "book_overview",
    "dated_page",
    "media_links",
    "media_category",
    "download_media",
    "media_downloads",
    "remove_media",
    "catalog_cached",
    "check_updates",
    "load_catalog",
    "home_lists",
    "whats_new",
    "teaching_toolbox",
    "categories",
    "category",
    "meetings",
    "referenced_publications",
    "missing_entry",
    "languages",
    "catalog_search",
    "catalog_languages",
    "favorite_entries",
    "download_publication",
    "page_user_data",
    "save_answer",
    "add_mark",
    "set_mark_color",
    "delete_mark",
    "save_note",
    "delete_note",
    "all_notes",
    "tags",
    "bookmarks",
    "user_data_summary",
    "open_location",
    "export_backup",
    "restore_backup",
    "merge_backup",
    "running_jobs",
];

#[derive(serde::Deserialize)]
struct Dir {
    dir: String,
}

#[derive(serde::Deserialize)]
struct Chapter {
    dir: String,
    book: i64,
    chapter: i64,
}

#[derive(serde::Deserialize)]
struct Verse {
    dir: String,
    book: i64,
    chapter: i64,
    verse: i64,
}

#[derive(serde::Deserialize)]
struct Book {
    dir: String,
    book: i64,
}

#[derive(serde::Deserialize)]
struct Lang {
    lang: String,
}

#[derive(serde::Deserialize)]
struct LangDate {
    lang: String,
    date: String,
}

#[derive(serde::Deserialize)]
struct Guid {
    guid: String,
}

#[derive(serde::Deserialize)]
struct PathArg {
    path: String,
}

#[derive(serde::Deserialize)]
struct TargetArg {
    target: Target,
}

fn parse<A: DeserializeOwned>(name: &str, args: Value) -> Result<A, String> {
    serde_json::from_value(args).map_err(|e| format!("{name}: {e}"))
}

/// Parse the arguments, run the command and serialize its result.
fn run<A, R>(name: &str, args: Value, f: impl FnOnce(A) -> ApiResult<R>) -> Result<Value, String>
where
    A: DeserializeOwned,
    R: Serialize,
{
    let value = f(parse(name, args)?)?;
    serde_json::to_value(value).map_err(|e| format!("{name}: {e}"))
}

/// A command without arguments; `{}` and `null` both do, anything else but an object is refused.
fn run0<R: Serialize>(
    name: &str,
    args: Value,
    f: impl FnOnce() -> ApiResult<R>,
) -> Result<Value, String> {
    if !(args.is_null() || args.is_object()) {
        return Err(format!("{name}: takes no arguments"));
    }
    serde_json::to_value(f()?).map_err(|e| format!("{name}: {e}"))
}

impl Core {
    /// Call a portable command by name with a JSON object of the arguments the web UI sends.
    pub fn invoke(&self, name: &str, args: Value) -> Result<Value, String> {
        match name {
            "cache_usage" => run0(name, args, || self.cache_usage()),
            "clear_cache" => run0(name, args, || self.clear_cache()),
            "list_publications" => run0(name, args, || self.list_publications()),
            "publication" => run(name, args, |a: Dir| self.publication(&a.dir)),
            "render_page" => run(name, args, |a: TargetArg| self.render_page(&a.target)),
            "link_action" => {
                #[derive(serde::Deserialize)]
                struct A {
                    current: Option<String>,
                    href: String,
                }
                run(name, args, |a: A| {
                    self.link_action(a.current.as_deref(), &a.href)
                })
            }
            "year_text" => {
                #[derive(serde::Deserialize)]
                struct A {
                    lang: String,
                    year: i64,
                }
                run(name, args, |a: A| self.year_text(&a.lang, a.year))
            }
            "remove_publication" => run(name, args, |a: Dir| self.remove_publication(&a.dir)),
            "import_files" => {
                #[derive(serde::Deserialize)]
                struct A {
                    paths: Vec<String>,
                }
                run(name, args, |a: A| self.import_files(a.paths))
            }
            "book_abbreviations" => run(name, args, |a: Dir| self.book_abbreviations(&a.dir)),
            "last_verse" => run(name, args, |a: Chapter| {
                self.last_verse(&a.dir, a.book, a.chapter)
            }),
            "research_verses" => run(name, args, |a: Chapter| {
                self.research_verses(&a.dir, a.book, a.chapter)
            }),
            "research_guide" => run(name, args, |a: Verse| {
                self.research_guide(&a.dir, a.book, a.chapter, a.verse)
            }),
            "chapter_study" => run(name, args, |a: Chapter| {
                self.chapter_study(&a.dir, a.book, a.chapter)
            }),
            "book_overview" => run(name, args, |a: Book| self.book_overview(&a.dir, a.book)),
            "dated_page" => {
                #[derive(serde::Deserialize)]
                struct A {
                    lang: String,
                    kind: String,
                    date: String,
                }
                run(name, args, |a: A| {
                    self.dated_page(&a.lang, &a.kind, &a.date)
                })
            }
            "media_links" => {
                #[derive(serde::Deserialize)]
                struct A {
                    media: MediaRef,
                }
                run(name, args, |a: A| self.media_links(&a.media))
            }
            "media_category" => {
                #[derive(serde::Deserialize)]
                struct A {
                    lang: String,
                    key: String,
                    detailed: bool,
                }
                run(name, args, |a: A| {
                    self.media_category(&a.lang, &a.key, a.detailed)
                })
            }
            "download_media" => {
                #[derive(serde::Deserialize)]
                struct A {
                    lang: String,
                    category: String,
                    detailed: bool,
                    key: String,
                    label: String,
                }
                run(name, args, |a: A| {
                    self.download_media(&a.lang, &a.category, a.detailed, &a.key, &a.label)
                })
            }
            "media_downloads" => run0(name, args, || self.media_downloads()),
            "remove_media" => {
                #[derive(serde::Deserialize)]
                struct A {
                    id: i64,
                }
                run(name, args, |a: A| self.remove_media(a.id))
            }
            "catalog_cached" => run0(name, args, || self.catalog_cached()),
            "check_updates" => run0(name, args, || self.check_updates()),
            "load_catalog" => run0(name, args, || self.load_catalog()),
            "home_lists" => run(name, args, |a: LangDate| self.home_lists(&a.lang, &a.date)),
            "whats_new" => {
                #[derive(serde::Deserialize)]
                struct A {
                    langs: Vec<String>,
                }
                run(name, args, |a: A| self.whats_new(&a.langs))
            }
            "teaching_toolbox" => run(name, args, |a: Lang| self.teaching_toolbox(&a.lang)),
            "categories" => run(name, args, |a: Lang| self.categories(&a.lang)),
            "category" => {
                #[derive(serde::Deserialize)]
                struct A {
                    lang: String,
                    id: i64,
                }
                run(name, args, |a: A| self.category(&a.lang, a.id))
            }
            "meetings" => run(name, args, |a: LangDate| self.meetings(&a.lang, &a.date)),
            "referenced_publications" => {
                #[derive(serde::Deserialize)]
                struct A {
                    dir: String,
                    lang: String,
                }
                run(name, args, |a: A| {
                    self.referenced_publications(&a.dir, &a.lang)
                })
            }
            "missing_entry" => {
                #[derive(serde::Deserialize)]
                struct A {
                    lang: String,
                    href: String,
                }
                run(name, args, |a: A| self.missing_entry(&a.lang, &a.href))
            }
            "languages" => run0(name, args, || self.languages()),
            "catalog_search" => {
                #[derive(serde::Deserialize)]
                struct A {
                    lang: String,
                    query: String,
                }
                run(name, args, |a: A| self.catalog_search(&a.lang, &a.query))
            }
            "catalog_languages" => {
                #[derive(serde::Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct A {
                    symbol: String,
                    issue_tag: i64,
                }
                run(name, args, |a: A| {
                    self.catalog_languages(&a.symbol, a.issue_tag)
                })
            }
            "favorite_entries" => {
                #[derive(serde::Deserialize)]
                struct A {
                    lang: String,
                    keys: Vec<String>,
                }
                run(name, args, |a: A| self.favorite_entries(&a.lang, &a.keys))
            }
            "download_publication" => {
                #[derive(serde::Deserialize)]
                struct A {
                    item: CatalogItem,
                    lang: String,
                }
                run(name, args, |a: A| {
                    self.download_publication(&a.item, &a.lang)
                })
            }
            "page_user_data" => run(name, args, |a: TargetArg| self.page_user_data(&a.target)),
            "save_answer" => {
                #[derive(serde::Deserialize)]
                struct A {
                    target: Target,
                    tag: String,
                    value: String,
                }
                run(name, args, |a: A| {
                    self.save_answer(&a.target, &a.tag, &a.value)
                })
            }
            "add_mark" => {
                #[derive(serde::Deserialize)]
                struct A {
                    target: Target,
                    color: i64,
                    ranges: Vec<Range>,
                }
                run(name, args, |a: A| {
                    self.add_mark(&a.target, a.color, &a.ranges)
                })
            }
            "set_mark_color" => {
                #[derive(serde::Deserialize)]
                struct A {
                    guid: String,
                    color: i64,
                }
                run(name, args, |a: A| self.set_mark_color(&a.guid, a.color))
            }
            "delete_mark" => run(name, args, |a: Guid| self.delete_mark(&a.guid)),
            "save_note" => {
                #[derive(serde::Deserialize)]
                struct A {
                    target: Option<Target>,
                    note: NoteInput,
                }
                run(name, args, |a: A| {
                    self.save_note(a.target.as_ref(), &a.note)
                })
            }
            "delete_note" => run(name, args, |a: Guid| self.delete_note(&a.guid)),
            "all_notes" => {
                #[derive(serde::Deserialize)]
                struct A {
                    tag: Option<i64>,
                }
                run(name, args, |a: A| self.all_notes(a.tag))
            }
            "tags" => run0(name, args, || self.tags()),
            "bookmarks" => run0(name, args, || self.bookmarks()),
            "user_data_summary" => run0(name, args, || self.user_data_summary()),
            "open_location" => {
                #[derive(serde::Deserialize)]
                struct A {
                    loc: Loc,
                }
                run(name, args, |a: A| self.open_location(&a.loc))
            }
            "export_backup" => run(name, args, |a: PathArg| {
                self.export_backup(Path::new(&a.path))
            }),
            "restore_backup" => run(name, args, |a: PathArg| {
                self.restore_backup(Path::new(&a.path))
            }),
            "merge_backup" => run(name, args, |a: PathArg| {
                self.merge_backup(Path::new(&a.path))
            }),
            "running_jobs" => run0(name, args, || Ok(self.running_jobs())),
            _ => Err(format!("unknown command: {name}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::mpsc;
    use std::time::Duration;

    use serde_json::json;

    use super::common;
    use super::*;
    use crate::core::{Host, Paths};

    struct NoHost;

    impl Host for NoHost {
        fn emit(&self, _event: &str, _payload: Value) {}
    }

    struct Env {
        library: tempfile::TempDir,
        cache: tempfile::TempDir,
    }

    impl Env {
        fn new() -> Env {
            Env {
                library: tempfile::tempdir().unwrap(),
                cache: tempfile::tempdir().unwrap(),
            }
        }

        fn open(&self) -> Core {
            let paths = Paths {
                library: self.library.path().to_owned(),
                cache: self.cache.path().to_owned(),
            };
            Core::open(paths, Arc::new(NoHost)).unwrap()
        }
    }

    /// A core whose library holds one synthetic publication; returns it and the target of its
    /// first document.
    fn with_publication(env: &Env) -> (Core, Value) {
        let file = env.cache.path().join("tst_E.jwpub");
        common::Fixture::default().write_to(&file);
        let core = env.open();
        let imported = core
            .invoke("import_files", json!({ "paths": [file.to_str().unwrap()] }))
            .unwrap();
        assert_eq!(imported[0]["error"], Value::Null, "{imported}");
        // The synthetic publication has no `Multimedia` table, which `list_publications` needs.
        let dir = pergament::Library::open(env.library.path())
            .unwrap()
            .list()
            .unwrap()[0]
            .dir_name
            .clone();
        (
            core,
            json!({ "publication": dir, "kind": { "document": 1 } }),
        )
    }

    fn note(title: &str, content: &str) -> Value {
        json!({ "title": title, "content": content })
    }

    #[test]
    fn unknown_command_is_an_error() {
        let env = Env::new();
        let err = env.open().invoke("nope", json!({})).unwrap_err();
        assert!(err.contains("unknown command: nope"), "{err}");
    }

    #[test]
    fn bad_args_name_the_command() {
        let env = Env::new();
        let err = env
            .open()
            .invoke("publication", json!({ "dir": 3 }))
            .unwrap_err();
        assert!(err.starts_with("publication:"), "{err}");
    }

    /// The list of names cannot drift from the match: none of them may fall through.
    #[test]
    fn every_listed_command_is_dispatched() {
        let env = Env::new();
        let core = env.open();
        for name in COMMANDS {
            // These would reach the network with `{}`; an array is refused before they run.
            let args = if NETWORK.contains(name) {
                json!([])
            } else {
                json!({})
            };
            if let Err(e) = core.invoke(name, args) {
                assert!(!e.starts_with("unknown command:"), "{name}: {e}");
            }
        }
        assert!(
            core.invoke("open_external", json!({}))
                .unwrap_err()
                .starts_with("unknown command:")
        );
    }

    #[test]
    fn unicode_note_round_trips() {
        let env = Env::new();
        let (core, target) = with_publication(&env);
        let text = "Gott – „Liebe“ #1 café";
        core.invoke(
            "save_note",
            json!({ "target": target, "note": note(text, text) }),
        )
        .unwrap();
        let notes = core.invoke("all_notes", json!({ "tag": null })).unwrap();
        assert_eq!(notes.as_array().unwrap().len(), 1);
        assert_eq!(
            notes[0]["title"].as_str().unwrap().as_bytes(),
            text.as_bytes()
        );
        assert_eq!(
            notes[0]["content"].as_str().unwrap().as_bytes(),
            text.as_bytes()
        );
    }

    #[test]
    fn concurrent_writes_both_land() {
        let env = Env::new();
        let (core, target) = with_publication(&env);
        let core = Arc::new(core);
        let (tx, rx) = mpsc::channel();
        for n in 0..8 {
            let (core, target, tx) = (core.clone(), target.clone(), tx.clone());
            std::thread::spawn(move || {
                let saved = core.invoke(
                    "save_note",
                    json!({ "target": target, "note": note(&format!("note {n}"), "x") }),
                );
                tx.send(saved).unwrap();
            });
        }
        drop(tx);
        for _ in 0..8 {
            let saved = rx
                .recv_timeout(Duration::from_secs(10))
                .expect("a writer did not finish within 10 s");
            saved.unwrap();
        }
        let notes = core.invoke("all_notes", json!({ "tag": null })).unwrap();
        assert_eq!(notes.as_array().unwrap().len(), 8);
    }

    #[test]
    fn reopen_after_unclean_stop() {
        let env = Env::new();
        let (core, target) = with_publication(&env);
        core.invoke(
            "save_note",
            json!({ "target": target, "note": note("kept", "body") }),
        )
        .unwrap();
        let media = core.media_dir().unwrap();
        std::fs::create_dir_all(&media).unwrap();
        std::fs::write(media.join("half.mp4.part"), b"partial").unwrap();
        drop(core);

        let core = env.open();
        let notes = core.invoke("all_notes", json!({ "tag": null })).unwrap();
        assert_eq!(notes.as_array().unwrap().len(), 1);
        assert_eq!(notes[0]["title"], "kept");
        // `media_downloads` lists the database's media rows, not the directory, so a stray
        // partial file is invisible to it by construction.
        let media = core.invoke("media_downloads", Value::Null).unwrap();
        assert_eq!(media, json!([]));
    }
}
