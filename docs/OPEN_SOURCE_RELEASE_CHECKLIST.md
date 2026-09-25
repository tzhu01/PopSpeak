# Open-source release checklist

This is a blocking checklist, not a declaration that the current working tree
or portable binary is ready to publish.

## Source and secrets

- [ ] Decide and document the canonical public repository.
- [x] License decision: PopSpeak's original source uses the OSI-approved MIT
      License (SPDX `MIT`). Commercial use, forks and resale remain permitted
      under its terms; no source-available or field-of-use restriction is added.
      This decision covers source code, not third-party model/runtime licenses or
      permission to impersonate the official PopSpeak brand and signed builds.
- [ ] Rotate credentials ever pasted into chat, issue trackers or commits.
- [ ] Publish only the intended branch. Never push local checkpoint refs with
      `--all` or `--mirror`; scan every ref that will be published.
- [ ] Run `./scripts/audit-source-release.ps1 -PublishRef HEAD
      -RequireCleanWorktree` (optionally with a local `-DenylistPath`) and review
      every path-only finding. The default scan includes Git's untracked,
      non-ignored files but intentionally excludes ignored models/build output. See
      [SOURCE_RELEASE_AUDIT.md](SOURCE_RELEASE_AUDIT.md).
- [ ] Separately run Gitleaks in Git-history mode over every ref that will be
      published. The snapshot script and CI check do not inspect complete history.
- [ ] Confirm `git status` contains only reviewed, intentionally tracked files;
      never stage a local denylist.

## Reproducible clean build

- [ ] Clone the exact publish commit into a new temporary directory.
- [ ] Use `.node-version`, `rust-toolchain.toml` and `npm ci`.
- [ ] Select the reviewed release profile before downloading assets. For the
      lean SenseVoice NSIS installer, run `scripts/build-lean-nsis.ps1`, which
      fetches and verifies only its pinned assets. Do not fetch/copy Whisper,
      Fun-ASR or llama runtimes into this installer. A future full-model
      portable release needs its own license audit and build gate. No resource
      may be copied from a developer machine or an older portable directory.
- [ ] Run frontend tests/lint/format/build and Rust fmt/clippy/tests/audit.
- [ ] Build the NSIS installer from that clean clone; verify its SHA-256 and
      source commit. Install on a clean Windows VM and run
      `scripts/assert-lean-nsis-assets.ps1 -InstalledDir <path>` against the
      actual installed payload. A separate portable release must verify its
      own manifest.

## Models, runtimes and licenses

- [ ] Reconcile the package contents with `model-catalog.json`, download scripts,
      checksums, UI claims and `THIRD_PARTY_NOTICES.md`.
- [ ] Include the exact license/NOTICE text for every shipped model and native
      runtime, including transitive DLLs. Do not describe all models as MIT or
      Apache-2.0.
- [ ] Record immutable upstream revision, filename, size and SHA-256 for every
      downloaded artifact and source archive.
- [ ] Generate dependency license inventories and an SPDX/CycloneDX SBOM.
- [ ] Resolve redistribution blockers for the actual selected payload. The
      lean NSIS resource allowlist excludes the old `libomp`-dependent Whisper
      and llama runtimes; the old Windows portable snapshot still fails
      `scripts/assert-redistributable-runtime.ps1` and must not be uploaded.
      The four native GGUF model-download paths must remain disabled in this
      installer until their full applicable model-license texts and catalog
      have been audited; source adapters may remain for future work.

## Release integrity

- [ ] For a signed official release, build in CI, sign the app, installer and
      bundled native DLLs with a trusted Authenticode certificate, and verify
      signatures after packaging. A separately approved unsigned prerelease
      must be plainly labeled and must not claim to be officially signed.
- [ ] Attach checksums, SBOM, provenance and the source commit to the release.
- [ ] Test a fresh installation on clean Windows 10 and Windows 11 systems,
      including an offline local-recognition smoke test and a cloud opt-in test.
- [ ] Verify that the installed payload contains no settings, history,
      activation database, API credential, private key, transcript or raw
      recording. Never infer NSIS contents from the source tree alone.
