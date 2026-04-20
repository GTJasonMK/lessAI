# Fork Release Maintenance

This fork publishes its own GitHub Releases and its own Tauri updater feed.

## Current fork settings

- GitHub repository: `88lin/lessAI`
- Updater endpoint: `https://github.com/88lin/lessAI/releases/latest/download/latest.json`
- Stable base version in repo config: `0.2.7`

## Signing key

- Public key is stored in `src-tauri/tauri.conf.json`
- Private key should stay outside the repository
- Local private key path used on this machine:
  `C:\Users\Computer\.lessai-release\88lin\tauri-signing.key`

## GitHub Actions secrets

Set the updater signing key on the repository:

```powershell
$secret = Get-Content C:\Users\Computer\.lessai-release\88lin\tauri-signing.key -Raw
gh secret set TAURI_SIGNING_PRIVATE_KEY --repo 88lin/lessAI --body $secret
```

If you later rotate to a password-protected key, also set:

```powershell
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo 88lin/lessAI
```

## Automatic package publishing

When you push to `master`, GitHub Actions now:

1. Detects the current repo version from `src-tauri/tauri.conf.json`
2. Generates an automatic GitHub release tag like `v0.2.9.1`
3. Uses a Tauri-compatible internal app version like `0.2.9-1`
4. Builds Windows, Linux, and macOS installers
5. Publishes a GitHub Release with installers, signatures, `latest.json`, and checksums

This is the normal flow after you manually sync upstream.

Version line examples:

- If `src-tauri/tauri.conf.json` stays at `0.2.7`, automatic fork releases continue as `v0.2.9.1`, `v0.2.9.2`, `v0.2.9.3`
- If upstream later changes the repo version to `0.2.9`, automatic fork releases switch to `v0.2.10.1`, `v0.2.10.2`
- If upstream later changes the repo version to `0.2.10`, automatic fork releases switch to `v0.2.11.1`, `v0.2.11.2`

There is also a scheduled fallback every 15 minutes. If a `master` push does not start Actions immediately, GitHub will still detect the new HEAD and publish one automatic package release for that commit.

## Manual stable release

If you want a manually controlled version number, create and push a `v*` tag.

## Suggested commands

Normal sync and auto-package flow:

```powershell
git fetch upstream --tags
git checkout master
git merge --ff-only upstream/master
git push origin master
```

Optional manual stable release:

```powershell
git tag v0.2.7
git push origin v0.2.7
```
