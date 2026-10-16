//! Updating the app itself from its GitHub releases: finding a newer signed
//! release for the way the app was installed, checking its signature and
//! putting it in place.

use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};

/// The minisign key the release workflow signs updates with (base64, as Tauri writes it).
pub const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEE5NTJCRjMwMzRCRjhDNjUKUldSbGpMODBNTDlTcWRKOW9tOEhpcHlkYmtJWjNFemtLajBUZXlyTWNub3BxNWloTjlaY2RNdUQK";

pub const RELEASES_URL: &str = "https://api.github.com/repos/v3rm1n0/pergament/releases/latest";

/// Where the release notes GitHub generates start; the install text in front is skipped.
const NOTES_MARKER: &str = "<!-- changes -->";

/// How the running app was installed, which decides whether it can replace itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallKind {
    /// Built by the flake; Nix manages the files.
    Nix,
    /// The AppImage file at this path.
    AppImage(PathBuf),
    /// The release zip's `install.sh`, with both binaries in this writable directory.
    Zip { bin_dir: PathBuf },
    /// Anything the app cannot update itself: packages, flatpak, macOS, a root-owned prefix.
    Manual,
}

/// `appimage` and `appdir` are `$APPIMAGE` and `$APPDIR`. Both are inherited by
/// child processes, so the AppImage only counts when the app runs from inside its mount.
pub fn install_kind(
    nix: bool,
    appimage: Option<&Path>,
    appdir: Option<&Path>,
    exe: &Path,
) -> InstallKind {
    if nix {
        return InstallKind::Nix;
    }
    if let (Some(image), Some(appdir)) = (appimage, appdir)
        && exe.starts_with(appdir)
    {
        return match image.parent() {
            Some(dir) if image.is_file() && writable(dir) => {
                InstallKind::AppImage(image.to_owned())
            }
            _ => InstallKind::Manual,
        };
    }
    let Some(bin_dir) = exe.parent() else {
        return InstallKind::Manual;
    };
    if exe.file_name().is_some_and(|n| n == "pergament-app")
        && bin_dir.join("pergament").is_file()
        && !cargo_output(bin_dir)
        && writable(bin_dir)
    {
        InstallKind::Zip {
            bin_dir: bin_dir.to_owned(),
        }
    } else {
        InstallKind::Manual
    }
}

/// `target/debug`, `target/release` or `target/<triple>/release`: a build, not an install.
fn cargo_output(dir: &Path) -> bool {
    dir.file_name()
        .is_some_and(|n| n == "debug" || n == "release")
        && dir
            .ancestors()
            .skip(1)
            .take(2)
            .any(|a| a.file_name().is_some_and(|n| n == "target"))
}

/// Tried rather than read from the mode bits, which miss ACLs, read-only mounts and root.
fn writable(dir: &Path) -> bool {
    tempfile::NamedTempFile::new_in(dir).is_ok()
}

pub fn current_install_kind() -> InstallKind {
    let appimage = env::var_os("APPIMAGE").map(PathBuf::from);
    let appdir = env::var_os("APPDIR").map(PathBuf::from);
    // Cached before `main`, so it still names the installed file after an update replaced it.
    match tauri::utils::platform::current_exe() {
        Ok(exe) => install_kind(
            option_env!("PERGAMENT_UPDATER") == Some("nix"),
            appimage.as_deref(),
            appdir.as_deref(),
            &exe,
        ),
        Err(_) => InstallKind::Manual,
    }
}

/// `$APPIMAGE` inherited from another AppImage (an editor or a terminal) that started this app.
pub fn stray_appimage() -> bool {
    let Some(appdir) = env::var_os("APPDIR") else {
        return env::var_os("APPIMAGE").is_some();
    };
    env::var_os("APPIMAGE").is_some()
        && !tauri::utils::platform::current_exe().is_ok_and(|exe| exe.starts_with(appdir))
}

/// `v1.2.3`, `1.2.3+build` or `1.2.3-rc1` as numbers; the suffix is ignored.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.strip_prefix('v').unwrap_or(s);
    let core = s.split(['+', '-']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let v = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(v)
}

pub fn is_newer(latest: &str, running: &str) -> bool {
    match (parse_version(latest), parse_version(running)) {
        (Some(l), Some(r)) => l > r,
        _ => false,
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub body: Option<String>,
    pub html_url: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
}

pub fn release_notes(body: &str) -> &str {
    body.split_once(NOTES_MARKER)
        .map_or(body, |(_, notes)| notes)
        .trim()
}

/// The download for `kind` and its signature, if the release has both.
pub fn assets_for<'r>(
    release: &'r Release,
    kind: &InstallKind,
    arch: &str,
) -> Option<(&'r Asset, &'r Asset)> {
    let version = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name);
    let name = match kind {
        InstallKind::Zip { .. } => format!("pergament-{version}-linux-{arch}.zip"),
        InstallKind::AppImage(_) => format!("Pergament-{version}-linux-{arch}.AppImage"),
        InstallKind::Nix | InstallKind::Manual => return None,
    };
    let find = |n: &str| release.assets.iter().find(|a| a.name == n);
    Some((find(&name)?, find(&format!("{name}.sig"))?))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Available {
    pub version: String,
    pub notes: String,
    /// The release page, for installs the app cannot update itself.
    pub url: String,
    pub installable: bool,
}

pub fn available(
    release: &Release,
    running: &str,
    kind: &InstallKind,
    arch: &str,
) -> Option<Available> {
    if !is_newer(&release.tag_name, running) {
        return None;
    }
    Some(Available {
        version: release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name)
            .to_owned(),
        notes: release_notes(release.body.as_deref().unwrap_or_default()).to_owned(),
        url: release.html_url.clone(),
        installable: assets_for(release, kind, arch).is_some(),
    })
}

/// Base64 of minisign text, as Tauri writes keys and `.sig` files.
fn decode_text(b64: &str, what: &str) -> Result<String, String> {
    let bytes = STANDARD
        .decode(b64.trim())
        .map_err(|e| format!("{what} is not base64: {e}"))?;
    String::from_utf8(bytes).map_err(|_| format!("{what} is not text"))
}

/// Check `path` against a signature made with the key `public_key_b64` for a
/// file called `name`. Tauri signs the name in the trusted comment
/// (`timestamp:…\tfile:<name>`), so a genuine older release cannot be passed
/// off under a newer release's file name.
pub fn verify_file(
    path: &Path,
    name: &str,
    sig_b64: &str,
    public_key_b64: &str,
) -> Result<(), String> {
    let key = PublicKey::decode(&decode_text(public_key_b64, "the public key")?)
        .map_err(|e| format!("bad public key: {e}"))?;
    let sig = Signature::decode(&decode_text(sig_b64, "the signature")?)
        .map_err(|e| format!("bad signature: {e}"))?;
    let mut verifier = key
        .verify_stream(&sig)
        .map_err(|e| format!("signature does not match: {e}"))?;
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        verifier.update(&buf[..n]);
    }
    verifier
        .finalize()
        .map_err(|e| format!("signature does not match: {e}"))?;
    let signed_for = sig
        .trusted_comment()
        .split('\t')
        .find_map(|field| field.strip_prefix("file:"));
    if signed_for == Some(name) {
        Ok(())
    } else {
        Err(format!(
            "the signature is for {}, not {name}",
            signed_for.unwrap_or("an unnamed file")
        ))
    }
}

/// Write `contents` to a temp file next to `dest` and rename it over `dest`,
/// so `dest` is always either the old or the whole new file. Replacing a
/// running binary this way is safe on Linux.
pub fn replace_file(dest: &Path, contents: &mut dyn Read) -> io::Result<()> {
    let dir = dest
        .parent()
        .ok_or_else(|| io::Error::other("no parent directory"))?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    io::copy(contents, &mut tmp)?;
    tmp.flush()?;
    tmp.as_file().sync_all()?;
    fs::set_permissions(tmp.path(), fs::Permissions::from_mode(0o755))?;
    tmp.persist(dest).map_err(|e| e.error)?;
    Ok(())
}

/// The bytes of `<dir>/bin/<name>` in the release zip.
fn zip_binary(archive: &mut zip::ZipArchive<File>, name: &str) -> Result<Vec<u8>, String> {
    let suffix = format!("/bin/{name}");
    let index = (0..archive.len())
        .find(|&i| {
            archive
                .name_for_index(i)
                .is_some_and(|n| n.ends_with(&suffix))
        })
        .ok_or_else(|| format!("the download has no bin/{name}"))?;
    let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// Put a verified download in place of the installed files.
pub fn install(kind: &InstallKind, download: &Path) -> Result<(), String> {
    match kind {
        InstallKind::Zip { bin_dir } => {
            let file = File::open(download).map_err(|e| e.to_string())?;
            let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            // Both are read before anything is written, so a broken zip changes nothing.
            let cli = zip_binary(&mut archive, "pergament")?;
            let app = zip_binary(&mut archive, "pergament-app")?;
            // The app last: if the first replacement fails, the old app still runs.
            replace_file(&bin_dir.join("pergament"), &mut cli.as_slice())
                .map_err(|e| e.to_string())?;
            replace_file(&bin_dir.join("pergament-app"), &mut app.as_slice())
                .map_err(|e| e.to_string())
        }
        InstallKind::AppImage(image) => {
            let mut file = File::open(download).map_err(|e| e.to_string())?;
            replace_file(image, &mut file).map_err(|e| e.to_string())
        }
        InstallKind::Nix | InstallKind::Manual => {
            Err("this installation cannot be updated from the app".to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> Asset {
        Asset {
            name: name.to_owned(),
            browser_download_url: format!(
                "https://github.com/v3rm1n0/pergament/releases/download/v1.7.0/{name}"
            ),
        }
    }

    fn release(names: &[&str]) -> Release {
        Release {
            tag_name: "v1.7.0".to_owned(),
            body: Some("install text\n<!-- changes -->\n## What's Changed".to_owned()),
            html_url: "https://github.com/v3rm1n0/pergament/releases/tag/v1.7.0".to_owned(),
            assets: names.iter().map(|n| asset(n)).collect(),
        }
    }

    const ALL: &[&str] = &[
        "pergament-1.7.0-linux-x86_64.zip",
        "pergament-1.7.0-linux-x86_64.zip.sig",
        "Pergament-1.7.0-linux-x86_64.AppImage",
        "Pergament-1.7.0-linux-x86_64.AppImage.sig",
    ];

    fn zip_kind() -> InstallKind {
        InstallKind::Zip {
            bin_dir: PathBuf::from("/home/u/.local/bin"),
        }
    }

    #[test]
    fn compares_versions_ignoring_build_metadata() {
        assert!(is_newer("v1.7.0", "1.6.0+0ad029f"));
        assert!(!is_newer("v1.6.0", "1.6.0+0ad029f"));
        assert!(!is_newer("v1.5.9", "1.6.0"));
        assert!(is_newer("v1.10.0", "1.9.3"));
        assert!(!is_newer("nightly", "1.6.0"));
        assert_eq!(parse_version("v1.2.3-rc1"), Some((1, 2, 3)));
    }

    #[test]
    fn notes_start_after_the_marker() {
        assert_eq!(
            release_notes("install text\n<!-- changes -->\n## What's Changed\n* x"),
            "## What's Changed\n* x"
        );
        assert_eq!(release_notes("  ## Old release  "), "## Old release");
    }

    #[test]
    fn detects_install_kind() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let exe = bin.join("pergament-app");
        fs::write(&exe, "app").unwrap();
        // Without the command line tool next to it, it is not the zip install.
        assert!(matches!(
            install_kind(false, None, None, &exe),
            InstallKind::Manual
        ));
        fs::write(bin.join("pergament"), "cli").unwrap();
        assert!(
            matches!(install_kind(false, None, None, &exe), InstallKind::Zip { bin_dir } if bin_dir == bin)
        );
        assert!(matches!(
            install_kind(true, None, None, &exe),
            InstallKind::Nix
        ));
        let other = dir.path().join("app");
        assert!(matches!(
            install_kind(false, None, None, &other),
            InstallKind::Manual
        ));
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o555)).unwrap();
        let kind = install_kind(false, None, None, &exe);
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(kind, InstallKind::Manual));
    }

    /// An AppImage at `<dir>/apps/Pergament.AppImage`, mounted at `<dir>/.mount_x` with the app inside.
    fn appimage_setup(dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
        let apps = dir.join("apps");
        fs::create_dir(&apps).unwrap();
        let image = apps.join("Pergament.AppImage");
        fs::write(&image, "image").unwrap();
        let appdir = dir.join(".mount_x");
        fs::create_dir_all(appdir.join("usr/bin")).unwrap();
        let exe = appdir.join("usr/bin/pergament-app");
        fs::write(&exe, "app").unwrap();
        (image, appdir, exe)
    }

    #[test]
    fn detects_a_running_appimage() {
        let dir = tempfile::tempdir().unwrap();
        let (image, appdir, exe) = appimage_setup(dir.path());
        assert_eq!(
            install_kind(false, Some(&image), Some(&appdir), &exe),
            InstallKind::AppImage(image.clone())
        );
    }

    /// `APPIMAGE` is inherited, so an app started from another AppImage (an editor, a terminal) sees that one's.
    #[test]
    fn ignores_an_appimage_it_is_not_running_from() {
        let dir = tempfile::tempdir().unwrap();
        let (image, appdir, _) = appimage_setup(dir.path());
        let bin = dir.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let exe = bin.join("pergament-app");
        fs::write(&exe, "app").unwrap();
        fs::write(bin.join("pergament"), "cli").unwrap();
        assert_eq!(
            install_kind(false, Some(&image), Some(&appdir), &exe),
            InstallKind::Zip {
                bin_dir: bin.clone()
            }
        );
        assert_eq!(
            install_kind(false, Some(&image), None, &exe),
            InstallKind::Zip { bin_dir: bin }
        );
    }

    #[test]
    fn leaves_an_appimage_in_a_read_only_directory_alone() {
        let dir = tempfile::tempdir().unwrap();
        let (image, appdir, exe) = appimage_setup(dir.path());
        let apps = image.parent().unwrap();
        fs::set_permissions(apps, fs::Permissions::from_mode(0o555)).unwrap();
        let kind = install_kind(false, Some(&image), Some(&appdir), &exe);
        fs::set_permissions(apps, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(kind, InstallKind::Manual);
    }

    /// `cargo build` puts both binaries next to each other too; those are not an install.
    #[test]
    fn does_not_update_a_cargo_build() {
        let dir = tempfile::tempdir().unwrap();
        for profile in ["debug", "release"] {
            let bin = dir.path().join("target").join(profile);
            fs::create_dir_all(&bin).unwrap();
            fs::write(bin.join("pergament"), "cli").unwrap();
            let exe = bin.join("pergament-app");
            fs::write(&exe, "app").unwrap();
            assert_eq!(
                install_kind(false, None, None, &exe),
                InstallKind::Manual,
                "{profile}"
            );
        }
    }

    #[test]
    fn picks_signed_assets_for_the_kind() {
        let r = release(ALL);
        let (file, sig) = assets_for(&r, &zip_kind(), "x86_64").unwrap();
        assert_eq!(file.name, "pergament-1.7.0-linux-x86_64.zip");
        assert_eq!(sig.name, "pergament-1.7.0-linux-x86_64.zip.sig");
        let image = InstallKind::AppImage(PathBuf::from("/x.AppImage"));
        let (file, sig) = assets_for(&r, &image, "x86_64").unwrap();
        assert_eq!(file.name, "Pergament-1.7.0-linux-x86_64.AppImage");
        assert_eq!(sig.name, "Pergament-1.7.0-linux-x86_64.AppImage.sig");
        assert!(assets_for(&r, &InstallKind::Manual, "x86_64").is_none());
        assert!(assets_for(&r, &InstallKind::Nix, "x86_64").is_none());
        assert!(assets_for(&r, &zip_kind(), "aarch64").is_none());
        let unsigned = release(&ALL[..1]);
        assert!(assets_for(&unsigned, &zip_kind(), "x86_64").is_none());
    }

    #[test]
    fn offers_only_newer_releases() {
        let r = release(ALL);
        let a = available(&r, "1.6.0+abc", &zip_kind(), "x86_64").unwrap();
        assert_eq!(a.version, "1.7.0");
        assert_eq!(a.notes, "## What's Changed");
        assert_eq!(a.url, r.html_url);
        assert!(a.installable);
        let unsigned = release(&ALL[..1]);
        assert!(
            !available(&unsigned, "1.6.0", &zip_kind(), "x86_64")
                .unwrap()
                .installable
        );
        assert!(available(&r, "1.7.0", &zip_kind(), "x86_64").is_none());
    }

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/update")
            .join(name)
    }

    fn read(name: &str) -> String {
        fs::read_to_string(fixture(name)).unwrap()
    }

    /// A zip install with the old binaries.
    fn old_install() -> (tempfile::TempDir, InstallKind) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("pergament"), "old-cli").unwrap();
        fs::write(dir.path().join("pergament-app"), "old-app").unwrap();
        let kind = InstallKind::Zip {
            bin_dir: dir.path().to_owned(),
        };
        (dir, kind)
    }

    #[test]
    fn accepts_a_good_signature() {
        let r = verify_file(
            &fixture("update.zip"),
            "update.zip",
            &read("update.zip.sig"),
            &read("test.key.pub"),
        );
        assert_eq!(r, Ok(()));
    }

    #[test]
    fn rejects_a_tampered_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut bytes = fs::read(fixture("update.zip")).unwrap();
        bytes[100] ^= 1;
        let path = dir.path().join("update.zip");
        fs::write(&path, bytes).unwrap();
        assert!(
            verify_file(
                &path,
                "update.zip",
                &read("update.zip.sig"),
                &read("test.key.pub")
            )
            .is_err()
        );
    }

    /// The signature names the file it was made for, so an older signed build cannot pose as a newer one.
    #[test]
    fn rejects_a_signature_for_another_file() {
        let r = verify_file(
            &fixture("update.zip"),
            "pergament-9.9.9-linux-x86_64.zip",
            &read("update.zip.sig"),
            &read("test.key.pub"),
        );
        assert!(r.is_err());
    }

    #[test]
    fn rejects_another_key() {
        assert!(
            verify_file(
                &fixture("update.zip"),
                "update.zip",
                &read("update.zip.sig"),
                PUBLIC_KEY
            )
            .is_err()
        );
    }

    #[test]
    fn replaces_both_binaries_in_a_zip_install() {
        let (dir, kind) = old_install();
        install(&kind, &fixture("update.zip")).unwrap();
        for (name, text) in [("pergament", "new-cli"), ("pergament-app", "new-app")] {
            let path = dir.path().join(name);
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o755
            );
        }
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            2,
            "temp files left behind"
        );
    }

    #[test]
    fn leaves_the_install_alone_when_the_zip_lacks_a_binary() {
        let (dir, kind) = old_install();
        assert!(install(&kind, &fixture("missing-app.zip")).is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("pergament")).unwrap(),
            "old-cli"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("pergament-app")).unwrap(),
            "old-app"
        );
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            2,
            "temp files left behind"
        );
    }

    #[test]
    fn replaces_an_appimage() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("Pergament.AppImage");
        fs::write(&image, "old").unwrap();
        let download = dir.path().join("download");
        fs::write(&download, "new image").unwrap();
        install(&InstallKind::AppImage(image.clone()), &download).unwrap();
        assert_eq!(fs::read_to_string(&image).unwrap(), "new image");
        assert_eq!(
            fs::metadata(&image).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }

    #[test]
    fn does_not_install_what_it_cannot_update() {
        assert!(install(&InstallKind::Manual, &fixture("update.zip")).is_err());
        assert!(install(&InstallKind::Nix, &fixture("update.zip")).is_err());
    }
}
