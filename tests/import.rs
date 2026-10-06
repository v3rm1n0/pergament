mod common;

use std::path::Path;

use common::Fixture;
use pergament::{Error, Library};

fn import(
    fx: &Fixture,
) -> (
    tempfile::TempDir,
    Library,
    pergament::Result<pergament::Entry>,
) {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("in.jwpub");
    fx.write_to(&file);
    let mut lib = Library::open(tmp.path().join("lib")).unwrap();
    let res = lib.import(&file);
    (tmp, lib, res)
}

/// Nothing but the index and the publications dir may remain after a failure.
fn assert_library_empty(lib: &Library) {
    assert!(lib.list().unwrap().is_empty());
    let leftovers: Vec<_> = std::fs::read_dir(lib.root().join("publications"))
        .unwrap()
        .collect();
    assert!(leftovers.is_empty(), "leftovers: {leftovers:?}");
}

#[test]
fn imports_valid_publication() {
    let (_tmp, lib, res) = import(&Fixture::default());
    let entry = res.unwrap();
    assert_eq!(entry.symbol, common::SYMBOL);
    assert_eq!(entry.meps_language, common::LANG);
    assert_eq!(entry.dir_name, "tst_0");
    assert_eq!(entry.lang_code.as_deref(), Some("E"));
    assert!(lib.db_path(&entry).is_file());
    assert!(lib.publication_dir(&entry).join("img_1.jpg").is_file());
    assert_eq!(lib.list().unwrap(), vec![entry.clone()]);
    assert_eq!(lib.find("tst", Some(0), None).unwrap(), vec![entry]);
}

#[test]
fn reimport_replaces_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open(tmp.path().join("lib")).unwrap();

    let first = tmp.path().join("a.jwpub");
    Fixture {
        title: "First".into(),
        extra_entries: vec![("old.jpg".into(), b"old".to_vec())],
        ..Fixture::default()
    }
    .write_to(&first);
    lib.import(&first).unwrap();

    let second = tmp.path().join("b.jwpub");
    Fixture {
        title: "Second".into(),
        extra_entries: vec![("new.jpg".into(), b"new".to_vec())],
        ..Fixture::default()
    }
    .write_to(&second);
    let entry = lib.import(&second).unwrap();

    let all = lib.list().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].title, "Second");
    let dir = lib.publication_dir(&entry);
    assert!(dir.join("new.jpg").is_file());
    assert!(
        !dir.join("old.jpg").exists(),
        "stale files survived re-import"
    );
}

#[test]
fn issues_are_separate_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open(tmp.path().join("lib")).unwrap();
    for issue in ["20240100", "20240200"] {
        let f = tmp.path().join(format!("{issue}.jwpub"));
        Fixture {
            issue: issue.into(),
            ..Fixture::default()
        }
        .write_to(&f);
        lib.import(&f).unwrap();
    }
    assert_eq!(lib.list().unwrap().len(), 2);
    assert_eq!(lib.find("tst", None, Some("20240200")).unwrap().len(), 1);
}

#[test]
fn rejects_contents_hash_mismatch() {
    let (_tmp, lib, res) = import(&Fixture {
        corrupt_hash: true,
        ..Fixture::default()
    });
    assert!(matches!(
        res,
        Err(Error::HashMismatch {
            what: "contents SHA-256",
            ..
        })
    ));
    assert_library_empty(&lib);
}

#[test]
fn rejects_db_hash_mismatch() {
    let (_tmp, lib, res) = import(&Fixture {
        corrupt_db_hash: true,
        ..Fixture::default()
    });
    assert!(matches!(
        res,
        Err(Error::HashMismatch {
            what: "database SHA-1",
            ..
        })
    ));
    assert_library_empty(&lib);
}

#[test]
fn rejects_missing_db() {
    let (_tmp, lib, res) = import(&Fixture {
        with_db: false,
        ..Fixture::default()
    });
    assert!(matches!(res, Err(Error::MissingEntry(_))), "{res:?}");
    assert_library_empty(&lib);
}

#[test]
fn rejects_db_without_document_table() {
    let (_tmp, lib, res) = import(&Fixture {
        with_document_table: false,
        ..Fixture::default()
    });
    assert!(matches!(res, Err(Error::InvalidDatabase { .. })), "{res:?}");
    assert_library_empty(&lib);
}

#[test]
fn rejects_db_that_is_not_sqlite() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture {
        with_db: false,
        db_file_name: Some("tst_E.db".into()),
        extra_entries: vec![("tst_E.db".into(), b"not a database at all".to_vec())],
        ..Fixture::default()
    };
    // The manifest DB hash is computed from the real DB, so this fails on
    // the SHA-1 check or on opening; either way it must not be imported.
    let file = tmp.path().join("x.jwpub");
    fx.write_to(&file);
    let mut lib = Library::open(tmp.path().join("lib")).unwrap();
    assert!(lib.import(&file).is_err());
    assert_library_empty(&lib);
}

#[test]
fn rejects_path_traversal() {
    for name in [
        "../evil.txt",
        "a/../../evil.txt",
        "/abs/evil.txt",
        "a\\..\\evil.txt",
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let (_t, lib, res) = import(&Fixture {
            extra_entries: vec![(name.into(), b"pwned".to_vec())],
            ..Fixture::default()
        });
        assert!(matches!(res, Err(Error::UnsafePath(_))), "{name}: {res:?}");
        assert_library_empty(&lib);
        assert!(!tmp.path().join("evil.txt").exists());
        assert!(!Path::new("/abs/evil.txt").exists());
    }
}

#[test]
fn rejects_unsafe_db_file_name() {
    let (_tmp, lib, res) = import(&Fixture {
        db_file_name: Some("../tst_E.db".into()),
        ..Fixture::default()
    });
    assert!(matches!(res, Err(Error::UnsafePath(_))), "{res:?}");
    assert_library_empty(&lib);
}

#[test]
fn rejects_colliding_entries() {
    // The zip crate already refuses exact duplicate names, so collide a file
    // with a directory of the same name instead.
    let (_tmp, lib, res) = import(&Fixture {
        extra_entries: vec![
            ("dup".into(), b"one".to_vec()),
            ("dup/x.jpg".into(), b"two".to_vec()),
        ],
        ..Fixture::default()
    });
    assert!(res.is_err());
    assert_library_empty(&lib);
}

#[test]
fn rejects_non_zip() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("x.jwpub");
    std::fs::write(&file, b"this is not a zip file").unwrap();
    let mut lib = Library::open(tmp.path().join("lib")).unwrap();
    assert!(matches!(lib.import(&file), Err(Error::Zip(_))));
    assert_library_empty(&lib);
}

#[test]
fn rejects_missing_manifest() {
    let (_tmp, lib, res) = import(&Fixture {
        with_manifest: false,
        ..Fixture::default()
    });
    assert!(matches!(res, Err(Error::MissingEntry(ref e)) if e == "manifest.json"));
    assert_library_empty(&lib);
}

#[test]
fn rejects_oversize_expansion() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("x.jwpub");
    Fixture {
        extra_entries: vec![("big.bin".into(), vec![0u8; 64 * 1024])],
        ..Fixture::default()
    }
    .write_to(&file);
    let mut lib =
        Library::open(tmp.path().join("lib"))
            .unwrap()
            .with_limits(pergament::jwpub::Limits {
                max_expanded_size: 32 * 1024,
                ..Default::default()
            });
    assert!(matches!(lib.import(&file), Err(Error::TooLarge(_))));
    assert_library_empty(&lib);
}
