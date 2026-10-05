//! Turn decoded publication HTML into clean, sanitized HTML.
//!
//! Decoded content is untrusted. It is first rewritten (links, images,
//! footnote and cross-reference markers) and then passed through an
//! allow-list sanitizer, as is everything else that ends up in the output.
//!
//! Links are rewritten to the `jwlinux:` scheme so a viewer can route them:
//! - `jwlinux://bible/{book}:{ch}:{v}-{book}:{ch}:{v}` for Bible references,
//! - `jwlinux://pub/{lang}:{meps_document_id}/…` for publication links.

use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::Path;

use lol_html::html_content::{Element, TextChunk};
use lol_html::{RewriteStrSettings, element, rewrite_str, text};

use crate::reader::{Chapter, Publication, VerseRef};
use crate::{Error, Result};

#[derive(Debug, Clone, Default)]
pub struct RenderOptions {
    /// URL prefix for images, ending in `/`, e.g. `file:///…/publications/nwtsty_2/`.
    /// Images are dropped when `None`.
    pub media_base: Option<String>,
    /// Wrap the result in a complete HTML page with styles.
    pub standalone: bool,
}

pub struct Renderer<'a> {
    publication: &'a Publication,
    options: RenderOptions,
    sanitizer: ammonia::Builder<'static>,
}

/// Markers found while rewriting: `(number, marker text)`.
#[derive(Debug, Default)]
struct Markers {
    footnotes: Vec<(i64, String)>,
    citations: Vec<(i64, String)>,
}

impl<'a> Renderer<'a> {
    pub fn new(publication: &'a Publication, options: RenderOptions) -> Self {
        Self {
            publication,
            options,
            sanitizer: sanitizer(),
        }
    }

    /// Render a document by `DocumentId`.
    pub fn document(&self, id: i64) -> Result<String> {
        let (info, html) = self.publication.document(id)?;
        let Some(html) = html else {
            return Err(Error::NotFound(format!(
                "document {id} has no content{}",
                if self.publication.is_bible() {
                    " (Bible books are rendered per chapter, e.g. `1:1`)"
                } else {
                    ""
                }
            )));
        };
        let (mut body, markers) = self.rewrite(&html)?;
        // Most documents carry their footnotes inline; add any that are missing.
        let missing: Vec<_> = markers
            .footnotes
            .iter()
            .filter(|(n, _)| !body.contains(&format!("id=\"footnote{n}\"")))
            .cloned()
            .collect();
        body.push_str(&self.footnotes_section(id, &missing)?);
        Ok(self.finish(&info.title, &body))
    }

    /// Render one Bible chapter with footnotes, cross references and study notes.
    pub fn chapter(&self, book: i64, chapter: i64) -> Result<String> {
        let ch = self.publication.chapter(book, chapter)?;
        let title = format!("{} {}", ch.book.chapter_title, ch.number);
        let mut body = String::from("<article class=\"bible-chapter\">");
        if let Some(pre) = &ch.pre_content {
            body.push_str(&self.rewrite(pre)?.0);
        }
        let _ = write!(body, "<h2 class=\"chapter-title\">{}</h2>", escape(&title));
        let (content, markers) = self.rewrite(&ch.content)?;
        body.push_str(&content);
        if let Some(post) = &ch.post_content {
            body.push_str(&self.rewrite(post)?.0);
        }
        body.push_str("</article>");

        if let Some(doc) = ch.book.book_document_id {
            body.push_str(&self.footnotes_section(doc, &markers.footnotes)?);
            body.push_str(&self.citations_section(doc, &markers.citations)?);
        }
        body.push_str(&self.study_notes_section(&ch)?);
        Ok(self.finish(&title, &body))
    }

    fn finish(&self, title: &str, body: &str) -> String {
        let clean = self.sanitizer.clean(body).to_string();
        if self.options.standalone {
            page(title, &clean)
        } else {
            clean
        }
    }

    fn footnotes_section(&self, document_id: i64, markers: &[(i64, String)]) -> Result<String> {
        let mut out = String::new();
        let mut seen = HashSet::new();
        for (n, letter) in markers {
            if !seen.insert(*n) {
                continue;
            }
            let Some(html) = self.publication.footnote(document_id, *n)? else {
                continue;
            };
            let _ = write!(
                out,
                "<div class=\"footnote\"><a class=\"fn-back\" href=\"#footnotesource{n}\">{}</a>{}</div>",
                escape(letter.trim()),
                self.rewrite(&html)?.0
            );
        }
        Ok(section("footnotes", "Footnotes", &out))
    }

    fn citations_section(&self, document_id: i64, markers: &[(i64, String)]) -> Result<String> {
        let mut out = String::new();
        let mut seen = HashSet::new();
        for (n, letter) in markers {
            if !seen.insert(*n) {
                continue;
            }
            let mut refs = Vec::new();
            for (first, last) in self.publication.citations(document_id, *n)? {
                refs.push(self.reference_link(first, last)?);
            }
            if refs.is_empty() {
                continue;
            }
            let _ = write!(
                out,
                "<p class=\"xref\" id=\"xref{n}\"><a class=\"xr-back\" href=\"#xrefsource{n}\">{}</a> {}</p>",
                escape(letter.trim()),
                refs.join("; ")
            );
        }
        Ok(section("xrefs", "Cross references", &out))
    }

    fn study_notes_section(&self, ch: &Chapter) -> Result<String> {
        let mut out = String::new();
        for note in self
            .publication
            .study_notes(ch.first_verse_id, ch.last_verse_id)?
        {
            let _ = write!(
                out,
                "<div class=\"study-note\" id=\"note{}\">{}{}</div>",
                note.verse_id,
                self.rewrite(&note.label)?.0,
                self.rewrite(&note.content)?.0
            );
        }
        Ok(section("study-notes", "Study notes", &out))
    }

    /// `<a href="jwlinux://bible/…">Psalm 23:1-3</a>` for a verse id range.
    fn reference_link(&self, first: i64, last: i64) -> Result<String> {
        let a = self.publication.verse_ref(first)?;
        let b = self.publication.verse_ref(last)?;
        let name = self.publication.bible_book(a.book)?.chapter_title;
        let label = if a == b {
            format!("{name} {}:{}", a.chapter, a.verse)
        } else if a.book == b.book && a.chapter == b.chapter {
            format!("{name} {}:{}-{}", a.chapter, a.verse, b.verse)
        } else if a.book == b.book {
            format!("{name} {}:{}–{}:{}", a.chapter, a.verse, b.chapter, b.verse)
        } else {
            let other = self.publication.bible_book(b.book)?.chapter_title;
            format!(
                "{name} {}:{}–{other} {}:{}",
                a.chapter, a.verse, b.chapter, b.verse
            )
        };
        Ok(format!(
            "<a class=\"b\" href=\"{}\">{}</a>",
            bible_url(a, b),
            escape(&label)
        ))
    }

    /// Rewrite links, images and markers in one HTML fragment.
    fn rewrite(&self, html: &str) -> Result<(String, Markers)> {
        let markers = RefCell::new(Markers::default());
        let media_base = self.options.media_base.as_deref();
        let out = rewrite_str(
            html,
            RewriteStrSettings::new()
                // Empty tooltip anchors and print page markers.
                .append_element_content_handler(element!("span.tt, span.pageNum", |el| {
                    el.remove();
                    Ok(())
                }))
                .append_element_content_handler(element!("span[data-fnid]", |el| {
                    let n = int_attr(el, "data-fnid");
                    markers.borrow_mut().footnotes.push((n, String::new()));
                    to_marker_link(
                        el,
                        "fn",
                        &format!("footnote{n}"),
                        &format!("footnotesource{n}"),
                    )
                }))
                .append_element_content_handler(text!("span[data-fnid]", |t: &mut TextChunk| {
                    if let Some(last) = markers.borrow_mut().footnotes.last_mut() {
                        last.1.push_str(t.as_str());
                    }
                    Ok(())
                }))
                .append_element_content_handler(element!("span[data-mid]", |el| {
                    let n = int_attr(el, "data-mid");
                    markers.borrow_mut().citations.push((n, String::new()));
                    to_marker_link(el, "xr", &format!("xref{n}"), &format!("xrefsource{n}"))
                }))
                .append_element_content_handler(text!("span[data-mid]", |t: &mut TextChunk| {
                    if let Some(last) = markers.borrow_mut().citations.last_mut() {
                        last.1.push_str(t.as_str());
                    }
                    Ok(())
                }))
                .append_element_content_handler(element!("a[href]", |el| {
                    match el.get_attribute("href").as_deref().and_then(rewrite_href) {
                        Some(href) => el.set_attribute("href", &href)?,
                        None => el.remove_attribute("href"),
                    }
                    Ok(())
                }))
                .append_element_content_handler(element!("img", |el| {
                    let src = el.get_attribute("src");
                    match (media_base, src.as_deref().and_then(media_name)) {
                        (Some(base), Some(name)) => {
                            el.set_attribute("src", &format!("{base}{name}"))?
                        }
                        _ => el.remove(),
                    }
                    Ok(())
                })),
        )
        .map_err(|e| Error::Decode(format!("HTML rewrite: {e}")))?;
        Ok((out, markers.into_inner()))
    }
}

fn to_marker_link(
    el: &mut Element<'_, '_>,
    class: &str,
    target: &str,
    id: &str,
) -> lol_html::HandlerResult {
    el.set_tag_name("a")?;
    let mut attrs: Vec<String> = el.attributes().iter().map(|a| a.name()).collect();
    attrs.retain(|a| a != "href");
    for a in attrs {
        el.remove_attribute(&a);
    }
    el.set_attribute("class", class)?;
    el.set_attribute("href", &format!("#{target}"))?;
    el.set_attribute("id", id)?;
    Ok(())
}

fn int_attr(el: &Element<'_, '_>, name: &str) -> i64 {
    el.get_attribute(name)
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0)
}

/// Map a link from publication content to a safe target, or `None` to drop it.
pub fn rewrite_href(href: &str) -> Option<String> {
    let href = href.trim();
    if let Some(frag) = href.strip_prefix('#') {
        return frag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            .then(|| href.to_owned());
    }
    if let Some(rest) = href.strip_prefix("jwpub://b/") {
        // `NWTR/40:1:1-40:1:1`
        let (_version, range) = rest.split_once('/')?;
        return (!range.is_empty()
            && range
                .chars()
                .all(|c| c.is_ascii_digit() || c == ':' || c == '-'))
        .then(|| format!("jwlinux://bible/{range}"));
    }
    if let Some(rest) = href.strip_prefix("jwpub://p/") {
        // `X:1001070005/` or `X:1001070144/1-1`
        return (!rest.is_empty()
            && rest
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '/' | '-')))
        .then(|| format!("jwlinux://pub/{rest}"));
    }
    if href.starts_with("https://") || href.starts_with("http://") {
        return Some(href.to_owned());
    }
    None
}

/// File name from `jwpub-media://NAME`, restricted to a safe plain name.
pub fn media_name(src: &str) -> Option<&str> {
    let name = src.trim().strip_prefix("jwpub-media://")?;
    (!name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
    .then_some(name)
}

fn bible_url(a: VerseRef, b: VerseRef) -> String {
    format!(
        "jwlinux://bible/{}:{}:{}-{}:{}:{}",
        a.book, a.chapter, a.verse, b.book, b.chapter, b.verse
    )
}

/// `file://` URL (with trailing slash) for a directory, percent-encoded.
pub fn file_url_for_dir(dir: &Path) -> String {
    let mut out = String::from("file://");
    for b in dir.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    if !out.ends_with('/') {
        out.push('/');
    }
    out
}

fn section(class: &str, heading: &str, inner: &str) -> String {
    if inner.is_empty() {
        String::new()
    } else {
        format!("<section class=\"{class}\"><h2>{heading}</h2>{inner}</section>")
    }
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn sanitizer() -> ammonia::Builder<'static> {
    let mut b = ammonia::Builder::default();
    b.add_tags([
        "article",
        "aside",
        "figcaption",
        "figure",
        "footer",
        "header",
        "main",
        "nav",
        "section",
    ])
    .add_generic_attributes(["id", "class"])
    .add_url_schemes(["jwlinux", "file"]);
    b
}

const CSS: &str = r#"
:root { color-scheme: light dark; --fg: #1d1d1f; --bg: #fdfdfb; --muted: #6b6b6b; --accent: #2b5d8c; --rule: #ddd; }
@media (prefers-color-scheme: dark) {
  :root { --fg: #e6e6e6; --bg: #1c1c1e; --muted: #9a9a9a; --accent: #8ab4e0; --rule: #3a3a3c; }
}
body { margin: 0; background: var(--bg); color: var(--fg); font: 1.1rem/1.6 "Noto Serif", Georgia, serif; }
main { max-width: 42rem; margin: 0 auto; padding: 1.5rem 1rem 4rem; }
h1, h2, h3 { font-family: "Noto Sans", system-ui, sans-serif; line-height: 1.25; }
a { color: var(--accent); text-decoration: none; }
a:hover { text-decoration: underline; }
img { max-width: 100%; height: auto; }
figure { margin: 1.5rem 0; }
.imgCredit { font-size: .8rem; color: var(--muted); }
.cl { float: left; font-size: 2.6em; line-height: 1; margin-right: .3rem; font-family: "Noto Sans", sans-serif; }
.vl { font-size: .7em; vertical-align: super; color: var(--muted); margin-right: .15rem; }
a.fn, a.xr { font-size: .7em; vertical-align: super; font-family: sans-serif; }
a.xr { color: var(--muted); }
section { border-top: 1px solid var(--rule); margin-top: 2rem; font-size: .95rem; }
section h2 { font-size: 1rem; text-transform: uppercase; letter-spacing: .05em; color: var(--muted); }
.footnote p, .study-note p { margin: .3rem 0; }
.fn-back, .xr-back { font-weight: bold; margin-right: .3rem; float: left; }
.study-note { margin-bottom: 1rem; }
aside, .boxContent { border-left: 3px solid var(--rule); padding-left: 1rem; }
"#;

fn page(title: &str, body: &str) -> String {
    format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src file: data:; style-src 'unsafe-inline'\">\
<title>{}</title><style>{CSS}</style></head>\n<body><main>\n{body}\n</main></body></html>\n",
        escape(title)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hrefs() {
        assert_eq!(
            rewrite_href("jwpub://b/NWTR/40:1:1-40:1:2").as_deref(),
            Some("jwlinux://bible/40:1:1-40:1:2")
        );
        assert_eq!(
            rewrite_href("jwpub://p/X:1001070144/1-1").as_deref(),
            Some("jwlinux://pub/X:1001070144/1-1")
        );
        assert_eq!(
            rewrite_href("#footnotesource1").as_deref(),
            Some("#footnotesource1")
        );
        assert!(rewrite_href("https://www.jw.org/finder?x=1").is_some());
        for bad in [
            "javascript:alert(1)",
            "jwpub://b/NWTR/1:1\"><script>",
            "file:///etc/passwd",
            "#a\"b",
            "data:text/html,x",
            "jwpub://p/../../x?y",
        ] {
            assert_eq!(rewrite_href(bad), None, "{bad}");
        }
    }

    #[test]
    fn media_names() {
        assert_eq!(media_name("jwpub-media://a_b-1.jpg"), Some("a_b-1.jpg"));
        for bad in [
            "jwpub-media://../x.jpg",
            "jwpub-media://a/b.jpg",
            "jwpub-media://.hidden",
            "http://x/a.jpg",
            "jwpub-media://",
        ] {
            assert_eq!(media_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn file_urls() {
        assert_eq!(
            file_url_for_dir(Path::new("/a b/ü")),
            "file:///a%20b/%C3%BC/"
        );
    }

    #[test]
    fn sanitizer_strips_active_content() {
        let s = sanitizer();
        let out = s
            .clean(r#"<p onclick="x()">a<script>alert(1)</script><a href="javascript:x">l</a><img src="file:///a.jpg" onerror="x"><iframe src="https://x"></iframe><style>p{}</style></p>"#)
            .to_string();
        for bad in [
            "onclick",
            "<script",
            "javascript:",
            "onerror",
            "<iframe",
            "<style",
        ] {
            assert!(!out.contains(bad), "{bad} in {out}");
        }
        assert!(out.contains(r#"src="file:///a.jpg""#));
    }
}
