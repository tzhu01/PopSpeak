# Changelog

This file records PopSpeak source milestones. A source-only GitHub Release contains
source archives, not an official Windows installer or portable app. See the
[release policy](docs/RELEASE_AND_BRANCHES_ZH.md).

## Unreleased

## 0.4.3 — 2026-09-25 (source-only; no Windows binary)

- Added local image-to-text recognition using installed Windows OCR language packs.
  Images can be selected or dropped into the app; results can be edited, formatted,
  and copied. Uncopied edits are guarded before replacement or navigation, and
  imported images are not uploaded to a PopSpeak service or added to voice history.
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
