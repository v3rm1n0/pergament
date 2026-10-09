//! The year text from the title page of the daily text booklet (`es26`):
//! `<p class="sn">Jahrestext „…“ (<a …>Matthäus 5:3</a>)</p>`.

/// The year's theme scripture, split for display.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct YearText {
    /// The scripture with the quotation marks of its language.
    pub text: String,
    /// "Matthäus 5:3".
    pub reference: Option<String>,
}

const OPENING: &[char] = &['„', '“', '"', '«', '»', '‚', '‘', '「', '『', '‹'];

/// Find the year text in the (decoded) content of a document. It is the first
/// paragraph of class `sn` that links to a verse.
pub fn find(content: &str) -> Option<YearText> {
    let mut rest = content;
    while let Some(start) = rest.find("<p ") {
        rest = &rest[start..];
        let end = rest.find("</p>")?;
        let paragraph = &rest[..end];
        rest = &rest[end + 4..];
        let open_end = paragraph.find('>')?;
        let (tag, inner) = paragraph.split_at(open_end + 1);
        if !has_class(tag, "sn") || !inner.contains("<a ") {
            continue;
        }
        let reference = last_link_text(inner);
        let plain = collapse(&strip_tags(inner));
        if plain.is_empty() {
            continue;
        }
        return Some(split(&plain, reference));
    }
    None
}

fn has_class(tag: &str, class: &str) -> bool {
    tag.split("class=\"")
        .nth(1)
        .and_then(|v| v.split('"').next())
        .is_some_and(|v| v.split_whitespace().any(|c| c == class))
}

/// Text of the last `<a …>…</a>` in `html`.
fn last_link_text(html: &str) -> Option<String> {
    let start = html.rfind("<a ")?;
    let after = &html[start..];
    let text_start = after.find('>')? + 1;
    let text_end = after.find("</a>")?;
    let text = collapse(&strip_tags(&after[text_start..text_end]));
    (!text.is_empty()).then_some(text)
}

fn split(plain: &str, reference: Option<String>) -> YearText {
    // Everything before the reference's parentheses is "label “text”"; the
    // label ("Jahrestext") is left out and the quotation marks are kept.
    let body = reference
        .as_deref()
        .and_then(|r| plain.rfind(r))
        .map_or(plain, |i| &plain[..i])
        .trim_end_matches(|c: char| c == '(' || c.is_whitespace());
    let text = body.find(OPENING).map_or(body, |i| &body[i..]);
    YearText {
        text: text.trim().to_owned(),
        reference,
    }
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    decode_entities(&out)
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let decoded = rest.find(';').filter(|e| *e <= 8).and_then(|e| {
            let c = match &rest[1..e] {
                "nbsp" => ' ',
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                n if n.starts_with("#x") => char::from_u32(u32::from_str_radix(&n[2..], 16).ok()?)?,
                n if n.starts_with('#') => char::from_u32(n[1..].parse().ok()?)?,
                _ => return None,
            };
            Some((c, e + 1))
        });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Collapse runs of whitespace (including no-break spaces) into single spaces.
fn collapse(s: &str) -> String {
    s.split(|c: char| c.is_whitespace() || c == '\u{a0}')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn german() {
        let html = r#"<header><h1>Titel</h1></header><p id="p3" class="sn">Jahrestext „Glücklich sind die, denen bewusst ist, dass sie Gott brauchen“ (<a href="jwpub://b/NWTR/40:5:3-40:5:3" class="b">Matthäus&nbsp;5:3</a>)</p><p class="si">x</p>"#;
        assert_eq!(
            find(html),
            Some(YearText {
                text: "„Glücklich sind die, denen bewusst ist, dass sie Gott brauchen“".into(),
                reference: Some("Matthäus 5:3".into()),
            })
        );
    }

    #[test]
    fn english_with_markup_inside() {
        let html = r#"<p class="si">other</p><p id="p3" class="sn">Yeartext “<em>Happy</em> are those &amp; more” (<a href="x">Matthew 5:3</a>)</p>"#;
        let y = find(html).unwrap();
        assert_eq!(y.text, "“Happy are those & more”");
        assert_eq!(y.reference.as_deref(), Some("Matthew 5:3"));
    }

    #[test]
    fn without_quotes_the_whole_text_is_kept() {
        let y = find(r#"<p class="sn">Vers (<a href="x">Mt 5:3</a>)</p>"#).unwrap();
        assert_eq!(y.text, "Vers");
    }

    #[test]
    fn ignores_paragraphs_without_a_verse_link() {
        assert_eq!(
            find(r#"<p class="sn">Nur Text</p><p class="si">x <a href="y">z</a></p>"#),
            None
        );
        assert_eq!(find("no paragraphs"), None);
    }
}
