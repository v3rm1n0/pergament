use std::io::Write as _;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use pergament::catalog::Catalog;
use pergament::net::{Client, HttpConfig};
use pergament::remote::{self, Request};
use pergament::render::file_url_for_dir;
use pergament::{Entry, Library, Publication, RenderOptions, Renderer};

/// Unofficial reader for JW publications (.jwpub). Personal use only.
#[derive(Parser)]
#[command(name = "pergament", version)]
struct Cli {
    /// Library directory (default: $XDG_DATA_HOME/pergament)
    #[arg(long, global = true, env = "PERGAMENT_LIBRARY")]
    library: Option<PathBuf>,

    /// Cache directory for the catalog and downloads (default: $XDG_CACHE_HOME/pergament)
    #[arg(long, global = true, env = "PERGAMENT_CACHE")]
    cache: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Import .jwpub files into the library (re-importing replaces)
    Import {
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// List imported publications
    List,
    /// Render a document to HTML on stdout, or list documents if none given
    Show {
        /// Publication: symbol (e.g. nwtsty, wp26) or library directory name
        publication: String,
        /// DocumentId, or BOOK:CHAPTER for Bibles (e.g. 19:23)
        document: Option<String>,
        /// MEPS language index, if the symbol exists in several languages
        #[arg(long)]
        lang: Option<i64>,
        /// Emit only the HTML fragment, without page wrapper and styles
        #[arg(long)]
        fragment: bool,
    },
    /// Search the online catalog for publications in a language
    Search {
        /// Language code, e.g. X (German) or E (English)
        lang: String,
        /// Words to look for in titles and symbols (empty: list newest)
        query: Vec<String>,
        /// MEPS language id, for languages the code cannot be mapped for
        #[arg(long)]
        meps_id: Option<i64>,
        #[arg(long, default_value_t = 30)]
        limit: usize,
        /// Check for a newer catalog even if the cached one is recent
        #[arg(long)]
        refresh: bool,
    },
    /// Download a publication from jw.org and import it
    Download {
        /// Symbol, e.g. nwtsty, w, wp or a dated one like wp26
        publication: String,
        /// Language code, e.g. X
        #[arg(long)]
        lang: String,
        /// Issue tag for periodicals, e.g. 20260900 (default: newest in catalog)
        #[arg(long)]
        issue: Option<i64>,
        /// MEPS language id, if the language code cannot be mapped
        #[arg(long)]
        meps_id: Option<i64>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut library = match cli.library {
        Some(dir) => Library::open(dir),
        None => Library::open_default(),
    }
    .context("opening library")?;
    let cache = match cli.cache {
        Some(dir) => dir,
        None => pergament::library::app_dir(
            &dirs::cache_dir().context("no cache directory (set XDG_CACHE_HOME)")?,
        ),
    };

    match cli.command {
        Command::Search {
            lang,
            query,
            meps_id,
            limit,
            refresh,
        } => {
            let client = Client::new(HttpConfig::default());
            let catalog = Catalog::load(
                &client,
                &cache,
                refresh,
                Duration::from_secs(24 * 3600),
                &mut progress("catalog"),
            )?;
            let meps = resolve_language(&catalog, &lang, meps_id)?;
            let items = catalog.search(meps, &query.join(" "), limit)?;
            if items.is_empty() {
                println!("nothing found");
            }
            for i in items {
                let issue = if i.issue_tag > 0 {
                    format!("--issue {}", i.issue_tag)
                } else {
                    String::new()
                };
                println!(
                    "{:<8} {:<16} {:>6.1} MB  {}{}",
                    i.key_symbol,
                    issue,
                    i.size as f64 / 1e6,
                    i.title,
                    i.issue_title.map(|t| format!(" – {t}")).unwrap_or_default()
                );
            }
        }
        Command::Download {
            publication,
            lang,
            issue,
            meps_id,
        } => {
            let client = Client::new(HttpConfig::default());
            // Use the catalog for extra checks if one is cached; never force
            // a full catalog download just to fetch one file.
            let catalog = Catalog::open_cached(&cache)?;
            let item = match &catalog {
                Some(c) => {
                    let meps = resolve_language(c, &lang, meps_id)?;
                    let item = c.find(&publication, meps, issue)?;
                    if item.is_none() {
                        bail!(
                            "{publication} ({lang}{}) is not in the catalog, see `pergament search {lang}`",
                            issue.map(|i| format!(", issue {i}")).unwrap_or_default()
                        );
                    }
                    item
                }
                None => None,
            };
            let req = Request {
                key_symbol: item.as_ref().map_or(&publication, |i| &i.key_symbol),
                lang_code: &lang,
                issue_tag: item.as_ref().map(|i| i.issue_tag).or(issue),
            };
            let entry = remote::download(
                &client,
                &mut library,
                &cache.join("downloads"),
                &req,
                item.as_ref(),
                &mut progress(&publication),
            )?;
            println!("imported {}: {}", entry.dir_name, entry.title);
        }
        Command::Import { files } => {
            let mut failed = 0;
            for file in files {
                match library.import(&file) {
                    Ok(e) => println!("imported {} ({}): {}", e.dir_name, file.display(), e.title),
                    Err(err) => {
                        failed += 1;
                        eprintln!("error: {}: {err}", file.display());
                    }
                }
            }
            if failed > 0 {
                bail!("{failed} file(s) failed to import");
            }
        }
        Command::List => {
            for e in library.list()? {
                println!("{:<20} {:>4}  {}", e.dir_name, e.year, e.title);
            }
        }
        Command::Show {
            publication,
            document,
            lang,
            fragment,
        } => {
            let entry = resolve(&library, &publication, lang)?;
            let pub_ = Publication::open(&library, &entry)?;
            match document {
                None => list_documents(&pub_)?,
                Some(doc) => {
                    let renderer = Renderer::new(
                        &pub_,
                        RenderOptions {
                            media_base: Some(file_url_for_dir(pub_.media_dir())),
                            standalone: !fragment,
                        },
                    );
                    let html = match doc.split_once(':') {
                        Some((b, c)) => renderer.chapter(
                            b.parse().context("book number")?,
                            c.parse().context("chapter number")?,
                        )?,
                        None => renderer.document(doc.parse().context("document id")?)?,
                    };
                    print!("{html}");
                }
            }
        }
    }
    Ok(())
}

fn resolve(library: &Library, name: &str, lang: Option<i64>) -> Result<Entry> {
    let by_dir = library.get_by_dir(name)?;
    if let Some(e) = by_dir.filter(|e| lang.is_none_or(|l| e.meps_language == l)) {
        return Ok(e);
    }
    let mut found = library.find(name, lang, None)?;
    match found.len() {
        0 => bail!("publication `{name}` is not in the library (see `pergament list`)"),
        1 => Ok(found.remove(0)),
        _ => {
            let names: Vec<_> = found.iter().map(|e| e.dir_name.as_str()).collect();
            bail!("`{name}` is ambiguous, use one of: {}", names.join(", "))
        }
    }
}

fn list_documents(pub_: &Publication) -> Result<()> {
    if pub_.is_bible() {
        println!("Bible books (show with BOOK:CHAPTER):");
        for b in pub_.bible_books()? {
            println!("  {:>2}  {} ({} chapters)", b.number, b.title, b.chapters);
        }
        println!();
    }
    println!("Documents (show with DocumentId):");
    for d in pub_.documents()? {
        if d.has_content {
            println!("  {:>4}  {}", d.id, d.title);
        }
    }
    Ok(())
}

fn resolve_language(catalog: &Catalog, code: &str, meps_id: Option<i64>) -> Result<i64> {
    if let Some(id) = meps_id {
        return Ok(id);
    }
    catalog.meps_language(code).with_context(|| {
        format!("language code {code:?} is not known from the catalog, pass --meps-id")
    })
}

/// Progress printer for stderr, updating at most once per percent.
fn progress(label: &str) -> impl FnMut(u64, Option<u64>) + use<> {
    let label = label.to_owned();
    let mut last = u64::MAX;
    move |done, total| {
        let mb = done as f64 / 1e6;
        let (key, line) = match total {
            Some(t) if t > 0 => {
                let pct = done * 100 / t;
                (
                    pct,
                    format!("{label}: {mb:.1}/{:.1} MB ({pct}%)", t as f64 / 1e6),
                )
            }
            _ => (done >> 20, format!("{label}: {mb:.1} MB")),
        };
        if key != last {
            last = key;
            let mut err = std::io::stderr();
            let _ = write!(err, "\r{line}   ");
            if total.is_some_and(|t| done >= t) {
                let _ = writeln!(err);
            }
        }
    }
}
