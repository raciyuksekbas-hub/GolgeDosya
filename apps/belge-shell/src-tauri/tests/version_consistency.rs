//! Tek canonical ürün sürümü.
//!
//! Sürüm birden fazla dosyada yazılıdır ve bunlar birbirinden bağımsız
//! değiştirilebilir: Tauri yapılandırması Info.plist'i ve DMG adını, workspace
//! Cargo sürümü Hakkında ekranını (`CARGO_PKG_VERSION`), package.json ve
//! lockfile kökü arayüz paketini belirler. Biri geride kalırsa kullanıcı iki
//! farklı sürüm görür. Bu test hepsinin aynı olduğunu ve bir geliştirme yer
//! tutucusu olmadığını denetler.

use std::path::{Path, PathBuf};

fn app_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR = apps/belge-shell/src-tauri
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("apps/belge-shell")
        .to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn json_version(path: &Path) -> serde_json::Value {
    serde_json::from_str(&read(path)).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// `[workspace.package]` bölümündeki `version = "..."`.
fn workspace_version(root: &Path) -> String {
    let text = read(&root.join("Cargo.toml"));
    let section = text
        .split("[workspace.package]")
        .nth(1)
        .expect("[workspace.package] bölümü");
    let section = section.split("\n[").next().unwrap_or(section);
    section
        .lines()
        .find_map(|l| {
            let l = l.trim();
            l.strip_prefix("version")
                .map(|rest| rest.trim_start())
                .and_then(|rest| rest.strip_prefix('='))
                .map(|rest| rest.trim().trim_matches('"').to_string())
        })
        .expect("workspace sürümü")
}

/// Cargo.lock'taki `name = "belge-shell"` paketinin sürümü.
fn locked_version(root: &Path, package: &str) -> String {
    let text = read(&root.join("Cargo.lock"));
    let marker = format!("name = \"{package}\"\nversion = \"");
    let start = text.find(&marker).expect("Cargo.lock paketi") + marker.len();
    text[start..].split('"').next().unwrap().to_string()
}

#[test]
fn every_product_version_source_reports_the_same_release() {
    let app = app_dir();
    let root = app.parent().and_then(Path::parent).expect("depo kökü");
    let tauri = json_version(&app.join("src-tauri/tauri.conf.json"));
    let package = json_version(&app.join("package.json"));
    let lock = json_version(&app.join("package-lock.json"));
    let dev_mock = read(&app.join("src/devMock.ts"));

    let product = env!("CARGO_PKG_VERSION").to_string();
    let sources = [
        ("CARGO_PKG_VERSION (Hakkında)", product.clone()),
        ("[workspace.package] version", workspace_version(root)),
        ("Cargo.lock belge-shell", locked_version(root, "belge-shell")),
        (
            "tauri.conf.json version (Info.plist, DMG)",
            tauri["version"].as_str().unwrap_or_default().to_string(),
        ),
        (
            "package.json version",
            package["version"].as_str().unwrap_or_default().to_string(),
        ),
        (
            "package-lock.json version",
            lock["version"].as_str().unwrap_or_default().to_string(),
        ),
        (
            "package-lock.json packages[\"\"].version",
            lock["packages"][""]["version"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        ),
    ];
    for (name, version) in &sources {
        assert_eq!(version, &product, "{name} farklı sürüm bildiriyor");
    }
    assert!(
        dev_mock.contains(&format!("version: \"{product}\"")),
        "geliştirme IPC taklidindeki app_info sürümü {product} değil"
    );
}

#[test]
fn the_product_version_is_not_a_development_placeholder() {
    let version = env!("CARGO_PKG_VERSION");
    let parts: Vec<u64> = version
        .split('.')
        .map(|p| p.parse().expect("sayısal semver"))
        .collect();
    assert_eq!(parts.len(), 3, "MAJOR.MINOR.PATCH bekleniyor: {version}");
    assert!(
        !matches!(parts.as_slice(), [0, 0, _]),
        "0.0.x geliştirme yer tutucusudur: {version}"
    );
}
