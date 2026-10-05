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
        assert!(matches!(
            Link::parse("https://www.jw.org/"),
            Some(Link::External(_))
        ));
        for bad in [
            "jwlinux://bible/99:1:1",
            "jwlinux://bible/x",
            "jwlinux://pub/X:abc/",
            "file:///etc/passwd",
            "javascript:x",
        ] {
            assert_eq!(Link::parse(bad), None, "{bad}");
        }
    }
}
