# Changelog

This file records PopSpeak source milestones. A source-only GitHub Release contains
source archives, not an official Windows installer or portable app. See the
[release policy](docs/RELEASE_AND_BRANCHES_ZH.md).

## 0.4.4 — 2026-09-25 (Windows installer candidate)

- Removed the experimental image-to-text feature and its Windows OCR dependency.
- Added a reproducible, SenseVoice-only Windows NSIS build with the offline
  WebView2 installer, pinned model/runtime hashes, and a packaged-asset manifest.
- Kept unavailable offline engines visible but disabled, and blocked unaudited
  optional GGUF model downloads in this public build.
- Updated Windows dependency auditing and release checks. The local installer is
  unsigned; a signed stable release requires a signing certificate and separate
  installed-app verification.

## 0.4.3 — 2026-09-25 (source-only; no Windows binary)

- Added local Windows image-to-text recognition with editable and copyable results;
  this feature was removed in the subsequent 0.4.4 candidate.
- Simplified the public README into a Chinese-first product introduction with an
  English counterpart and an explicit Windows binary-release status.
- Documented a single-long-lived-branch strategy and the signed-release gate.
- Limited and grouped routine Dependabot pull requests.
- Made Windows binary releases manual so a source-only version tag does not start
  the installer and portable build before its signing and redistribution gates pass.

## 0.4.2 — 2026-09-20 (source milestone; no public binary Release)

- Added the POP result editor for review, formatting, copy controls, pinning,
  language display, and optional correction learning.
- Added local scene presets for daily typing, direct chat input, and meeting notes.
- Labeled engine-specific professional-vocabulary behavior: decoder hints where
  supported, local normalization otherwise.
- Added a model-independent local preview side lane and kept the selected model's
  final recognition result authoritative.
- Expanded the account-free offline trial to 200 recordings or 20 minutes, whichever
  comes first.

Earlier development history remains available in Git. No Windows installer or
portable archive should be presented as an official Release until CI, redistribution,
signing, and clean-machine checks are complete.
