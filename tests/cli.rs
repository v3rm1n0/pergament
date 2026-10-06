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
