use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use reqwest::{
    header::{ACCEPT, USER_AGENT},
    Client, Proxy, Url,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{utils::config::BundleType, utils::platform::bundle_type, AppHandle};
use tauri_plugin_updater::UpdaterExt;

use crate::network_proxy::normalize_proxy_url;
use crate::storage;

const GITHUB_RELEASES_API_URL: &str =
    "https://api.github.com/repos/GTJasonMK/lessAI/releases?per_page=50";
const GITHUB_RELEASE_BY_TAG_API_URL_TEMPLATE: &str =
    "https://api.github.com/repos/GTJasonMK/lessAI/releases/tags/{tag}";
const RELEASE_MANIFEST_URL_TEMPLATE: &str =
    "https://github.com/GTJasonMK/lessAI/releases/download/{tag}/latest.json";
const SYSTEM_PACKAGE_MANIFEST_ASSET_NAME: &str = "system-packages.json";
const RELEASES_USER_AGENT: &str = "LessAI-VersionManager/1.0";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseVersionSummary {
    pub tag: String,
    pub version: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub html_url: String,
    pub published_at: Option<String>,
    pub prerelease: bool,
    pub updater_available: bool,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    html_url: String,
    published_at: Option<String>,
    draft: bool,
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SystemPackagesManifest {
    schema_version: u32,
    packages: Vec<SystemPackageManifestEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SystemPackageManifestEntry {
    name: String,
    kind: String,
    arch: String,
    sha256: String,
}

#[derive(Debug, Clone, Copy)]
enum SystemPackageKind {
    Deb,
    Rpm,
}

fn is_safe_release_tag_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_')
}

fn normalize_release_tag(tag: &str) -> Result<String, String> {
    let tag = tag.trim();
    if tag.is_empty() {
        return Err("版本号不能为空。".to_string());
    }

    let raw = tag
        .strip_prefix('v')
        .or_else(|| tag.strip_prefix('V'))
        .unwrap_or(tag);
    if raw.is_empty() {
        return Err("版本号不能为空。".to_string());
    }

    let normalized = format!("v{raw}");

    if !normalized.chars().all(is_safe_release_tag_char) {
        return Err("版本号包含非法字符，仅允许字母、数字、点、下划线和短横线。".to_string());
    }

    Ok(normalized)
}

fn normalize_version_from_tag(tag: &str) -> String {
    tag.trim_start_matches(['v', 'V'])
        .to_string()
}

fn current_system_package_kind() -> Option<SystemPackageKind> {
    match bundle_type() {
        Some(BundleType::Deb) => Some(SystemPackageKind::Deb),
        Some(BundleType::Rpm) => Some(SystemPackageKind::Rpm),
        _ => None,
    }
}

fn target_package_extension(kind: SystemPackageKind) -> &'static str {
    match kind {
        SystemPackageKind::Deb => ".deb",
        SystemPackageKind::Rpm => ".rpm",
    }
}

fn current_arch_aliases() -> Vec<String> {
    match std::env::consts::ARCH {
        "x86_64" => vec!["x86_64".to_string(), "amd64".to_string()],
        "aarch64" => vec!["aarch64".to_string(), "arm64".to_string()],
        "arm" => vec!["armv7".to_string(), "armhf".to_string(), "arm".to_string()],
        other => vec![other.to_ascii_lowercase()],
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn find_release_asset_by_name<'a>(
    assets: &'a [GithubReleaseAsset],
    name: &str,
) -> Option<&'a GithubReleaseAsset> {
    assets.iter().find(|asset| asset.name == name)
}

fn kind_as_manifest_str(kind: SystemPackageKind) -> &'static str {
    match kind {
        SystemPackageKind::Deb => "deb",
        SystemPackageKind::Rpm => "rpm",
    }
}

fn score_manifest_arch(arch: &str, aliases: &[String]) -> i32 {
    let normalized = arch.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return -1;
    }

    if let Some(index) = aliases.iter().position(|alias| alias == &normalized) {
        return 100 - index as i32;
    }

    if matches!(
        normalized.as_str(),
        "all" | "any" | "noarch" | "universal"
    ) {
        return 10;
    }

    -1
}

fn pick_manifest_package_entry(
    manifest: &SystemPackagesManifest,
    kind: SystemPackageKind,
) -> Option<&SystemPackageManifestEntry> {
    let target_kind = kind_as_manifest_str(kind);
    let aliases = current_arch_aliases();

    manifest
        .packages
        .iter()
        .filter(|entry| {
            entry.kind.trim().eq_ignore_ascii_case(target_kind)
                && entry
                    .name
                    .to_ascii_lowercase()
                    .ends_with(target_package_extension(kind))
        })
        .filter_map(|entry| {
            let score = score_manifest_arch(&entry.arch, &aliases);
            if score < 0 {
                None
            } else {
                Some((entry, score))
            }
        })
        .max_by_key(|(_, score)| *score)
        .map(|(entry, _)| entry)
}

fn sanitize_asset_file_name(name: &str) -> String {
    let mut sanitized = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }
    while sanitized.contains("..") {
        sanitized = sanitized.replace("..", "_");
    }
    while sanitized.starts_with('.') {
        sanitized.remove(0);
    }

    if sanitized.is_empty() {
        "lessai-update-package.bin".to_string()
    } else {
        sanitized
    }
}

fn prepare_download_path(tag: &str, file_name: &str) -> Result<PathBuf, String> {
    let mut dir = std::env::temp_dir();
    dir.push("lessai-system-update");
    let tag_hash = sha256_hex(tag.as_bytes());
    let cache_key = tag_hash.chars().take(16).collect::<String>();
    dir.push(cache_key);
    fs::create_dir_all(&dir).map_err(|error| format!("创建下载目录失败：{error}"))?;
    dir.push(file_name);
    Ok(dir)
}

fn command_exists(name: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name} >/dev/null 2>&1"))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

async fn download_release_asset_bytes(
    client: &Client,
    asset: &GithubReleaseAsset,
) -> Result<Vec<u8>, String> {
    let response = client
        .get(&asset.browser_download_url)
        .header(USER_AGENT, RELEASES_USER_AGENT)
        .send()
        .await
        .map_err(|error| format!("下载资产 {} 失败：{error}", asset.name))?;

    if !response.status().is_success() {
        return Err(format!(
            "下载资产 {} 失败：HTTP {}",
            asset.name,
            response.status()
        ));
    }

    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| format!("读取资产 {} 失败：{error}", asset.name))
}

fn parse_system_packages_manifest(bytes: &[u8]) -> Result<SystemPackagesManifest, String> {
    let manifest: SystemPackagesManifest = serde_json::from_slice(bytes)
        .map_err(|error| format!("解析 system-packages.json 失败：{error}"))?;
    if manifest.schema_version != 1 {
        return Err(format!(
            "不支持的 system-packages.json 版本：{}（当前仅支持 1）",
            manifest.schema_version
        ));
    }
    if manifest.packages.is_empty() {
        return Err("system-packages.json 不包含任何包条目。".to_string());
    }
    Ok(manifest)
}

fn run_pkexec_install(kind: SystemPackageKind, package_path: &Path) -> Result<(), String> {
    if !command_exists("pkexec") {
        return Err(
            "当前系统未找到 pkexec，无法弹出管理员授权。请安装 polkit 并确保图形会话有授权代理。"
                .to_string(),
        );
    }

    let script = match kind {
        SystemPackageKind::Deb => {
            r#"set -eu
pkg="$1"
if command -v apt-get >/dev/null 2>&1; then
  apt-get install -y "$pkg"
elif command -v apt >/dev/null 2>&1; then
  apt install -y "$pkg"
elif command -v dpkg >/dev/null 2>&1; then
  dpkg -i "$pkg"
else
  echo "No supported deb package manager found." >&2
  exit 127
fi"#
        }
        SystemPackageKind::Rpm => {
            r#"set -eu
pkg="$1"
if command -v dnf >/dev/null 2>&1; then
  dnf install -y "$pkg"
elif command -v yum >/dev/null 2>&1; then
  yum install -y "$pkg"
elif command -v zypper >/dev/null 2>&1; then
  zypper --non-interactive install "$pkg"
elif command -v rpm >/dev/null 2>&1; then
  rpm -Uvh --replacepkgs "$pkg"
else
  echo "No supported rpm package manager found." >&2
  exit 127
fi"#
        }
    };

    let status = Command::new("pkexec")
        .arg("/bin/sh")
        .arg("-c")
        .arg(script)
        .arg("lessai-system-update")
        .arg(package_path.as_os_str())
        .status()
        .map_err(|error| format!("启动系统安装器失败：{error}"))?;

    if !status.success() {
        return Err(format!("系统安装器执行失败（退出码：{status}）。"));
    }

    Ok(())
}

fn build_reqwest_client(proxy: Option<String>, timeout_secs: u64) -> Result<Client, String> {
    let mut builder = Client::builder().timeout(Duration::from_secs(timeout_secs));
    if let Some(proxy) = proxy {
        Url::parse(&proxy).map_err(|error| format!("代理地址无效：{error}"))?;
        let reqwest_proxy = Proxy::all(proxy).map_err(|error| format!("代理配置失败：{error}"))?;
        builder = builder.proxy(reqwest_proxy);
    }
    builder
        .build()
        .map_err(|error| format!("网络客户端初始化失败：{error}"))
}

fn resolve_effective_proxy(app: &AppHandle, proxy: Option<String>) -> Option<String> {
    if let Some(proxy) = proxy.as_deref().and_then(normalize_proxy_url) {
        return Some(proxy);
    }

    storage::load_settings(app)
        .ok()
        .and_then(|settings| normalize_proxy_url(&settings.update_proxy))
}

#[tauri::command]
pub async fn list_release_versions(
    app: AppHandle,
    proxy: Option<String>,
) -> Result<Vec<ReleaseVersionSummary>, String> {
    let client = build_reqwest_client(resolve_effective_proxy(&app, proxy), 15)?;
    let response = client
        .get(GITHUB_RELEASES_API_URL)
        .header(USER_AGENT, RELEASES_USER_AGENT)
        .header(ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| format!("拉取版本列表失败：{error}"))?;

    if !response.status().is_success() {
        return Err(format!("拉取版本列表失败：HTTP {}", response.status()));
    }

    let releases: Vec<GithubRelease> = response
        .json()
        .await
        .map_err(|error| format!("解析版本列表失败：{error}"))?;

    let mut result = Vec::with_capacity(releases.len());
    for release in releases.into_iter().filter(|item| !item.draft) {
        let Ok(tag) = normalize_release_tag(&release.tag_name) else {
            continue;
        };
        let updater_available = release
            .assets
            .iter()
            .any(|asset| asset.name.eq_ignore_ascii_case("latest.json"));
        result.push(ReleaseVersionSummary {
            version: normalize_version_from_tag(&tag),
            tag,
            name: release.name,
            body: release.body,
            html_url: release.html_url,
            published_at: release.published_at,
            prerelease: release.prerelease,
            updater_available,
        });
    }

    Ok(result)
}

#[tauri::command]
pub async fn switch_release_version(
    app: AppHandle,
    tag: String,
    proxy: Option<String>,
) -> Result<String, String> {
    if matches!(bundle_type(), Some(BundleType::Deb) | Some(BundleType::Rpm)) {
        return Err(
            "当前为 Linux Deb/Rpm 安装包，由系统包管理器维护，不支持应用内切换版本。请使用系统包管理器升级，或改用 AppImage 包。"
                .to_string(),
        );
    }

    let tag = normalize_release_tag(&tag)?;
    let endpoint = RELEASE_MANIFEST_URL_TEMPLATE.replace("{tag}", &tag);
    let endpoint = Url::parse(&endpoint).map_err(|error| format!("构建更新地址失败：{error}"))?;

    let mut builder = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|error| format!("配置版本更新源失败：{error}"))?
        .version_comparator(|current, remote| current != remote.version)
        .timeout(Duration::from_secs(20));

    if let Some(proxy) = resolve_effective_proxy(&app, proxy) {
        let proxy = Url::parse(&proxy).map_err(|error| format!("代理地址无效：{error}"))?;
        builder = builder.proxy(proxy);
    }

    let updater = builder
        .build()
        .map_err(|error| format!("初始化更新器失败：{error}"))?;

    let Some(update) = updater
        .check()
        .await
        .map_err(|error| format!("检查目标版本失败：{error}"))?
    else {
        return Err(format!(
            "未发现可安装版本：{tag}。请确认该发布包含 latest.json 与当前平台更新包。"
        ));
    };

    let installed_version = update.version.to_string();
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|error| format!("安装版本 {tag} 失败：{error}"))?;

    Ok(installed_version)
}

#[tauri::command]
pub async fn install_system_package_release(
    app: AppHandle,
    tag: String,
    proxy: Option<String>,
) -> Result<String, String> {
    let Some(package_kind) = current_system_package_kind() else {
        return Err("当前安装包类型不需要系统包管理器安装。".to_string());
    };

    let tag = normalize_release_tag(&tag)?;
    let client = build_reqwest_client(resolve_effective_proxy(&app, proxy), 30)?;
    let endpoint = GITHUB_RELEASE_BY_TAG_API_URL_TEMPLATE.replace("{tag}", &tag);
    let endpoint = Url::parse(&endpoint).map_err(|error| format!("构建发布查询地址失败：{error}"))?;

    let response = client
        .get(endpoint)
        .header(USER_AGENT, RELEASES_USER_AGENT)
        .header(ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| format!("查询目标版本失败：{error}"))?;

    if !response.status().is_success() {
        return Err(format!("查询目标版本失败：HTTP {}", response.status()));
    }

    let release: GithubRelease = response
        .json()
        .await
        .map_err(|error| format!("解析目标版本信息失败：{error}"))?;

    let manifest_asset = find_release_asset_by_name(&release.assets, SYSTEM_PACKAGE_MANIFEST_ASSET_NAME)
        .ok_or_else(|| {
            format!(
                "目标版本 {tag} 缺少 {SYSTEM_PACKAGE_MANIFEST_ASSET_NAME}，无法安全校验系统安装包。请手动下载并安装。"
            )
        })?;
    let manifest_bytes = download_release_asset_bytes(&client, manifest_asset).await?;
    let manifest = parse_system_packages_manifest(&manifest_bytes)?;
    let manifest_entry =
        pick_manifest_package_entry(&manifest, package_kind).ok_or_else(|| {
            format!(
                "目标版本 {tag} 未在 {SYSTEM_PACKAGE_MANIFEST_ASSET_NAME} 中声明当前架构的 {} 安装包。",
                target_package_extension(package_kind)
            )
        })?;
    let asset = find_release_asset_by_name(&release.assets, &manifest_entry.name).ok_or_else(|| {
        format!(
            "目标版本 {tag} 的系统包清单指向了不存在的资产：{}",
            manifest_entry.name
        )
    })?;

    let expected_sha256 = manifest_entry.sha256.trim().to_ascii_lowercase();
    if expected_sha256.len() != 64 || !expected_sha256.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(format!(
            "目标版本 {tag} 的系统包清单包含非法 sha256：{}",
            manifest_entry.sha256
        ));
    }

    let download_path = prepare_download_path(&tag, &sanitize_asset_file_name(&asset.name))?;

    let package_bytes = download_release_asset_bytes(&client, asset).await?;
    let actual_sha256 = sha256_hex(&package_bytes);
    if actual_sha256 != expected_sha256 {
        return Err(format!(
            "安装包完整性校验失败：期望 sha256={}, 实际 sha256={actual_sha256}。已阻止提权安装。",
            expected_sha256
        ));
    }

    fs::write(&download_path, &package_bytes)
        .map_err(|error| format!("保存安装包失败：{error}"))?;

    let install_path = download_path.clone();
    tauri::async_runtime::spawn_blocking(move || run_pkexec_install(package_kind, &install_path))
        .await
        .map_err(|error| format!("安装任务执行失败：{error}"))??;

    Ok(normalize_version_from_tag(&tag))
}

#[cfg(test)]
mod tests {
    use super::{
        current_arch_aliases, normalize_release_tag, pick_manifest_package_entry,
        sanitize_asset_file_name, sha256_hex, SystemPackageKind, SystemPackageManifestEntry,
        SystemPackagesManifest,
    };

    #[test]
    fn normalize_release_tag_rejects_path_separator() {
        assert!(normalize_release_tag("../v0.3.3").is_err());
    }

    #[test]
    fn normalize_release_tag_normalizes_uppercase_prefix() {
        assert_eq!(
            normalize_release_tag("V0.3.3").expect("normalized tag"),
            "v0.3.3"
        );
    }

    #[test]
    fn normalize_release_tag_rejects_plus_character() {
        assert!(normalize_release_tag("v0.3.3+build").is_err());
    }

    #[test]
    fn pick_manifest_package_entry_prefers_exact_arch() {
        let preferred_arch = current_arch_aliases()
            .first()
            .cloned()
            .unwrap_or_else(|| "any".to_string());
        let manifest = SystemPackagesManifest {
            schema_version: 1,
            packages: vec![
                SystemPackageManifestEntry {
                    name: "lessai-universal.deb".to_string(),
                    kind: "deb".to_string(),
                    arch: "all".to_string(),
                    sha256: "a".repeat(64),
                },
                SystemPackageManifestEntry {
                    name: "lessai-amd64.deb".to_string(),
                    kind: "deb".to_string(),
                    arch: preferred_arch.clone(),
                    sha256: "b".repeat(64),
                },
            ],
        };

        let picked = pick_manifest_package_entry(&manifest, SystemPackageKind::Deb)
            .expect("expected deb asset");
        assert_eq!(picked.arch, preferred_arch);
    }

    #[test]
    fn sanitize_asset_file_name_replaces_path_separators() {
        let sanitized = sanitize_asset_file_name("../LessAI 0.3.3 amd64.deb");
        assert!(!sanitized.contains('/'));
        assert!(!sanitized.contains(".."));
        assert!(sanitized.ends_with(".deb"));
    }

    #[test]
    fn sha256_hex_matches_known_value() {
        let digest = sha256_hex(b"abc");
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
