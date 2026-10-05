use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use jwlinux::render::file_url_for_dir;
use jwlinux::{Entry, Library, Publication, RenderOptions, Renderer};

/// Unofficial reader for JW publications (.jwpub). Personal use only.
#[derive(Parser)]
#[command(name = "jwl", version)]
struct Cli {
    /// Library directory (default: $XDG_DATA_HOME/jwlinux)
    #[arg(long, global = true, env = "JWL_LIBRARY")]
    library: Option<PathBuf>,

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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut library = match cli.library {
        Some(dir) => Library::open(dir),
        None => Library::open_default(),
    }
    .context("opening library")?;

    match cli.command {
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
        0 => bail!("publication `{name}` is not in the library (see `jwl list`)"),
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
