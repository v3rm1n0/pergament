mod common;

use common::Fixture;
use jwlinux::{Library, Publication, RenderOptions, Renderer};

fn render(html: &str, options: RenderOptions) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("in.jwpub");
    Fixture {
        documents: vec![(1, html.to_owned())],
        ..Fixture::default()
    }
    .write_to(&file);
    let mut lib = Library::open(tmp.path().join("lib")).unwrap();
    let entry = lib.import(&file).unwrap();
    let publication = Publication::open(&lib, &entry).unwrap();
    Renderer::new(&publication, options).document(1).unwrap()
}

#[test]
fn strips_active_content() {
    let out = render(
        r#"<p onclick="evil()">Hi<script>alert(1)</script>
           <a href="javascript:alert(1)">js</a>
           <img src="jwpub-media://img_1.jpg" onerror="evil()">
           <img src="jwpub-media://../../etc/passwd">
           <img src="https://tracker.example/x.gif">
           <iframe src="https://example.org"></iframe>
           <style>body{display:none}</style></p>"#,
        RenderOptions {
            media_base: Some("file:///lib/tst_0/".into()),
            standalone: false,
        },
    );
    for bad in [
        "onclick",
        "<script",
        "alert",
        "javascript:",
        "onerror",
        "passwd",
        "tracker",
        "<iframe",
        "<style",
    ] {
        assert!(!out.contains(bad), "{bad:?} survived: {out}");
    }
    assert!(
        out.contains(r#"src="file:///lib/tst_0/img_1.jpg""#),
        "{out}"
    );
}

#[test]
fn drops_images_without_media_base() {
    let out = render(
        r#"<figure><img src="jwpub-media://img_1.jpg" alt="x"></figure>"#,
        RenderOptions::default(),
    );
    assert!(!out.contains("<img"), "{out}");
}

#[test]
fn rewrites_links_and_footnote_markers() {
    let out = render(
        r##"<p id="p1" data-pid="1">Text<span data-fnid="1" class="fn">a<span id="footnotesource1" class="tt fn" data-rel-fnid="1"></span></span>
           <a href="jwpub://b/NWTR/19:23:1-19:23:3" class="b">Ps 23:1-3</a>
           <a href="jwpub://p/X:123/" class="xt" data-xtid="1">other</a></p>
           <div class="groupFootnote"><div id="footnote1" data-fnid="1" class="fn-ref"><p><a href="#footnotesource1" class="fn-symbol">a</a> Note</p></div></div>"##,
        RenderOptions::default(),
    );
    assert!(out.contains(r##"href="#footnote1""##), "{out}");
    assert!(out.contains(r#"id="footnotesource1""#), "{out}");
    assert!(out.contains("jwlinux://bible/19:23:1-19:23:3"), "{out}");
    assert!(out.contains("jwlinux://pub/X:123/"), "{out}");
    assert!(!out.contains("jwpub:"), "{out}");
    assert!(!out.contains("data-"), "{out}");
    // Footnote is inline, so no extra section is appended.
    assert!(!out.contains("class=\"footnotes\""), "{out}");
}

#[test]
fn standalone_page_has_csp_and_escaped_title() {
    let out = render(
        "<p>x</p>",
        RenderOptions {
            media_base: None,
            standalone: true,
        },
    );
    assert!(out.starts_with("<!DOCTYPE html>"));
    assert!(out.contains("Content-Security-Policy"));
    assert!(out.contains("<title>Doc</title>"));
}
