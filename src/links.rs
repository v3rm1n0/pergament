//! Parsing of the link targets produced by [`crate::render`], so a viewer can
//! route clicks.

/// Where a link in rendered content points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// `jwlinux://bible/B:C:V-B:C:V` (end optional).
    Bible { book: i64, chapter: i64, verse: i64 },
    /// `jwlinux://pub/{lang}:{meps_document_id}/…`
    Document {
        lang_code: String,
        meps_document_id: i64,
    },
    /// `jwlinux://verse/{lang}:{book_document}/{ch}:{v}[-…]`: a verse given by
    /// the MEPS id of its Bible book document. `jwlinux://note/…` is the same
    /// but points at the study note on that verse (`study_note`).
    BookVerse {
        lang_code: String,
        book_document: i64,
        chapter: i64,
        verse: i64,
        study_note: bool,
    },
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
        if let Some(rest) = uri.strip_prefix("jwlinux://bible/") {
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
        if let Some(rest) = uri.strip_prefix("jwlinux://pub/") {
            let (lang, tail) = rest.split_once(':')?;
            let id = tail.split('/').next()?.parse().ok()?;
            return Some(Self::Document {
                lang_code: lang.to_owned(),
                meps_document_id: id,
            });
        }
        let book_verse = uri
            .strip_prefix("jwlinux://note/")
            .map(|r| (r, true))
            .or_else(|| uri.strip_prefix("jwlinux://verse/").map(|r| (r, false)));
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
        if uri.starts_with("https://") || uri.starts_with("http://") {
            return Some(Self::External(uri.to_owned()));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        assert_eq!(
            Link::parse("jwlinux://bible/19:23:1-19:23:3"),
            Some(Link::Bible {
                book: 19,
                chapter: 23,
                verse: 1
            })
        );
        assert_eq!(
            Link::parse("jwlinux://bible/1:2"),
            Some(Link::Bible {
                book: 1,
                chapter: 2,
                verse: 1
            })
        );
        assert_eq!(
            Link::parse("jwlinux://pub/X:1001070144/1-1"),
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
            Link::parse("jwlinux://note/X:1001070145/8:38"),
            Some(Link::BookVerse {
                lang_code: "X".into(),
                book_document: 1001070145,
                chapter: 8,
                verse: 38,
                study_note: true
            })
        );
        assert_eq!(
            Link::parse("jwlinux://verse/X:1001070145/16-17:5"),
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
        for bad in [
            "jwlinux://bible/99:1:1",
            "jwlinux://bible/x",
            "jwlinux://pub/X:abc/",
            "jwlinux://note/X:1/x:1",
            "file:///etc/passwd",
            "javascript:x",
        ] {
            assert_eq!(Link::parse(bad), None, "{bad}");
        }
    }
}
