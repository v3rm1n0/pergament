mod common;

use std::process::Command;

use common::Fixture;

fn pergament(lib: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_pergament"))
        .arg("--library")
        .arg(lib)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn import_list_show() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = tmp.path().join("lib");
    let file = tmp.path().join("tst_E.jwpub");
    Fixture::default().write_to(&file);

    let out = pergament(&lib, &["import", file.to_str().unwrap()]);
    assert!(out.status.success(), "{out:?}");

    let out = pergament(&lib, &["list"]);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("tst_0") && stdout.contains("Test Publication"));

    let out = pergament(&lib, &["show", "tst"]);
    assert!(String::from_utf8(out.stdout).unwrap().contains("Doc"));

    let out = pergament(&lib, &["show", "tst", "1"]);
    assert!(out.status.success(), "{out:?}");
    let html = String::from_utf8(out.stdout).unwrap();
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("Hello"));

    let out = pergament(&lib, &["show", "tst", "1", "--fragment"]);
    assert!(!String::from_utf8(out.stdout).unwrap().contains("<!DOCTYPE"));
}

#[test]
fn import_failure_sets_exit_code() {
    let tmp = tempfile::tempdir().unwrap();
    let bad = tmp.path().join("bad.jwpub");
    std::fs::write(&bad, b"nope").unwrap();
    let out = pergament(&tmp.path().join("lib"), &["import", bad.to_str().unwrap()]);
    assert!(!out.status.success());
}

#[test]
fn unknown_publication_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let out = pergament(&tmp.path().join("lib"), &["show", "nope"]);
    assert!(!out.status.success());
}

#[test]
fn grep_indexes_and_finds_paragraphs() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = tmp.path().join("lib");
    let file = tmp.path().join("tst_E.jwpub");
    Fixture {
        documents: vec![(
            1,
            // The paragraph without an anchor keeps the search status `Incomplete`.
            r#"<p data-pid="12">Café search phrase</p><p data-pid="13">other words</p><p>no anchor</p>"#.into(),
        )],
        ..Fixture::default()
    }
    .write_to(&file);
    let out = pergament(&lib, &["import", file.to_str().unwrap()]);
    assert!(out.status.success(), "{out:?}");

    // The first search indexes the new publication.
    let out = pergament(&lib, &["grep", "cafe", "phrase"]);
    assert!(out.status.success(), "{out:?}");
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("indexing 1 publication")
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("tst_0 1  ¶12  Test Publication"),
        "{stdout}"
    );
    assert!(stdout.contains("    Café search phrase"), "{stdout}");
    assert!(!stdout.contains("other words"), "{stdout}");

    // Already indexed: no second indexing pass.
    let out = pergament(&lib, &["grep", "cafe", "--in", "tst"]);
    assert!(out.status.success(), "{out:?}");
    assert!(out.stderr.is_empty(), "{out:?}");
    assert!(String::from_utf8(out.stdout).unwrap().contains("¶12"));

    let out = pergament(&lib, &["grep", "cafe", "--bible"]);
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "nothing found\n");

    let out = pergament(&lib, &["grep", "\"cafe"]);
    assert!(!out.status.success());

    let out = pergament(&lib, &["grep", "cafe", "--in", "nope"]);
    assert!(!out.status.success());
}
