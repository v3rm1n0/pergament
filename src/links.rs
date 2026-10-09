//! Parsing of the link targets produced by [`crate::render`], so a viewer can
//! route clicks.

/// The two kinds of recording a publication can link to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Audio,
    Video,
}

/// A recording linked as `https://www.jw.org/finder?lank=pub-…&wtlocale=…`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MediaRef {
    /// Key symbol such as `mwbv` or `jwb-098`.
    pub pub_symbol: String,
    /// `YYYYMM` for periodicals, `None` for undated publications.
    pub issue: Option<String>,
    pub track: u32,
    pub kind: MediaKind,
    pub lang_code: String,
}

/// Where a link in rendered content points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// `pergament://bible/B:C:V-B:C:V` (end optional).
    Bible { book: i64, chapter: i64, verse: i64 },
    /// `pergament://pub/{lang}:{meps_document_id}/…`
    Document {
        lang_code: String,
        meps_document_id: i64,
    },
    /// `pergament://verse/{lang}:{book_document}/{ch}:{v}[-…]`: a verse given by
    /// the MEPS id of its Bible book document. `pergament://note/…` is the same
    /// but points at the study note on that verse (`study_note`).
    BookVerse {
        lang_code: String,
        book_document: i64,
        chapter: i64,
        verse: i64,
        study_note: bool,
    },
    /// A video or audio recording, played in the app.
    Media(MediaRef),
    /// http(s) link to open in a browser.
    External(String),
    /// `#id` within the current page.
    Fragment(String),
}

impl Link {
    pub fn parse(uri: &str) -> Option<Self> {
        let uri = uri.trim();
        if let Some(frag) = uri.strip_prefix('#') {
            return Some(Self::Fragment(frag.to_owned()));
        }
        if let Some(rest) = uri.strip_prefix("pergament://bible/") {
            let start = rest.split('-').next()?;
            let mut parts = start.split(':').map(|p| p.parse::<i64>().ok());
            let (book, chapter) = (parts.next()??, parts.next()??);
            let verse = parts.next().flatten().unwrap_or(1);
            return (1..=66).contains(&book).then_some(Self::Bible {
                book,
                chapter,
                verse,
            });
        }
        if let Some(rest) = uri.strip_prefix("pergament://pub/") {
            let (lang, tail) = rest.split_once(':')?;
            let id = tail.split('/').next()?.parse().ok()?;
            return Some(Self::Document {
                lang_code: lang.to_owned(),
                meps_document_id: id,
            });
        }
        let book_verse = uri
            .strip_prefix("pergament://note/")
            .map(|r| (r, true))
            .or_else(|| uri.strip_prefix("pergament://verse/").map(|r| (r, false)));
        if let Some((rest, study_note)) = book_verse {
            let (lang, tail) = rest.split_once(':')?;
            let (doc, position) = tail.split_once('/')?;
            // `8:38`, `8:38-8:40`, or rarely a bare chapter `16-17:5`.
            let start = position.split('-').next()?;
            let (chapter, verse) = match start.split_once(':') {
                Some((c, v)) => (c.parse().ok()?, v.parse().ok()?),
                None => (start.parse().ok()?, 1),
            };
            return Some(Self::BookVerse {
                lang_code: lang.to_owned(),
                book_document: doc.parse().ok()?,
                chapter,
                verse,
                study_note,
            });
        }
        if let Some(media) = parse_media(uri) {
            return Some(Self::Media(media));
        }
        if uri.starts_with("https://") || uri.starts_with("http://") {
            return Some(Self::External(uri.to_owned()));
        }
        None
    }
}

/// `https://www.jw.org/finder?lank=pub-mwbv_202609_1_VIDEO&wtlocale=X` and the
/// undated `pub-jwb-098_7_VIDEO`. Other `lank` links (articles, other
/// publications) are not media.
fn parse_media(uri: &str) -> Option<MediaRef> {
    let rest = uri
        .strip_prefix("https://www.jw.org/finder?")
        .or_else(|| uri.strip_prefix("https://jw.org/finder?"))?;
    let (mut lank, mut lang) = (None, None);
    for pair in rest.split('&') {
        match pair.split_once('=') {
            Some(("lank", v)) => lank = Some(v),
            Some(("wtlocale", v)) => lang = Some(v),
            _ => {}
        }
    }
    let body = lank?.strip_prefix("pub-")?;
    let (body, kind) = if let Some(b) = body.strip_suffix("_VIDEO") {
        (b, MediaKind::Video)
    } else {
        (body.strip_suffix("_AUDIO")?, MediaKind::Audio)
    };
    let token = |s: &str| {
        !s.is_empty() && s.len() <= 32 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    };
    let mut parts = body.split('_');
    let (pub_symbol, second, third) = (parts.next()?, parts.next()?, parts.next());
    if parts.next().is_some() || !token(pub_symbol) {
        return None;
    }
    let (issue, track) = match third {
        Some(track) => (Some(second), track),
        None => (None, second),
    };
    if issue
        .is_some_and(|i| !(i.len() == 6 || i.len() == 8) || !i.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let lang = lang.filter(|l| token(l))?;
    Some(MediaRef {
        pub_symbol: pub_symbol.to_owned(),
        issue: issue.map(str::to_owned),
        track: track.parse().ok().filter(|t| *t > 0)?,
        kind,
        lang_code: lang.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        assert_eq!(
            Link::parse("pergament://bible/19:23:1-19:23:3"),
            Some(Link::Bible {
                book: 19,
                chapter: 23,
                verse: 1
            })
        );
        assert_eq!(
            Link::parse("pergament://bible/1:2"),
            Some(Link::Bible {
                book: 1,
                chapter: 2,
                verse: 1
            })
        );
        assert_eq!(
            Link::parse("pergament://pub/X:1001070144/1-1"),
            Some(Link::Document {
                lang_code: "X".into(),
                meps_document_id: 1001070144
            })
        );
        assert_eq!(
            Link::parse("#footnote1"),
            Some(Link::Fragment("footnote1".into()))
        );
        assert_eq!(
            Link::parse("pergament://note/X:1001070145/8:38"),
            Some(Link::BookVerse {
                lang_code: "X".into(),
                book_document: 1001070145,
                chapter: 8,
                verse: 38,
                study_note: true
            })
        );
        assert_eq!(
            Link::parse("pergament://verse/X:1001070145/16-17:5"),
            Some(Link::BookVerse {
                lang_code: "X".into(),
                book_document: 1001070145,
                chapter: 16,
                verse: 1,
                study_note: false
            })
        );
        assert!(matches!(
            Link::parse("https://www.jw.org/"),
            Some(Link::External(_))
        ));
        assert!(matches!(
            Link::parse("https://www.jw.org/finder?lank=pub-mwbv_202609_1_AUDIO"),
            Some(Link::External(_))
        ));
    }

    #[test]
    fn parses_media() {
        assert_eq!(
            Link::parse("https://www.jw.org/finder?lank=pub-mwbv_202609_1_VIDEO&wtlocale=X"),
            Some(Link::Media(MediaRef {
                pub_symbol: "mwbv".into(),
                issue: Some("202609".into()),
                track: 1,
                kind: MediaKind::Video,
                lang_code: "X".into()
            }))
        );
        assert_eq!(
            Link::parse("https://www.jw.org/finder?wtlocale=E&lank=pub-jwb-098_7_AUDIO"),
            Some(Link::Media(MediaRef {
                pub_symbol: "jwb-098".into(),
                issue: None,
                track: 7,
                kind: MediaKind::Audio,
                lang_code: "E".into()
            }))
        );
        for other in [
            "https://www.jw.org/finder?lank=docid-1_VIDEO&wtlocale=X",
            "https://www.jw.org/finder?lank=pub-mwbv_20269_1_VIDEO&wtlocale=X",
            "https://www.jw.org/finder?lank=pub-mwbv_1_0_VIDEO&wtlocale=X",
            "https://www.jw.org/finder?lank=pub-a_b_c_d_VIDEO&wtlocale=X",
            "https://www.jw.org/finder?lank=pub-mwbv_1_PDF&wtlocale=X",
            "https://www.jw.org/finder?lank=pub-mwbv_1_VIDEO&wtlocale=X%26",
        ] {
            assert!(
                !matches!(Link::parse(other), Some(Link::Media(_))),
                "{other}"
            );
        }
        for bad in [
            "pergament://bible/99:1:1",
            "pergament://bible/x",
            "pergament://pub/X:abc/",
            "pergament://note/X:1/x:1",
            "file:///etc/passwd",
            "javascript:x",
        ] {
            assert_eq!(Link::parse(bad), None, "{bad}");
        }
    }
}
