# Changelog

This file records PopSpeak source milestones. A source version is not necessarily
an official, downloadable GitHub Release. See the [release policy](docs/RELEASE_AND_BRANCHES_ZH.md).

## Unreleased

- Simplified the public README into a Chinese-first product introduction with an
  English counterpart and an explicit Windows binary-release status.
- Documented a single-long-lived-branch strategy and the signed-release gate.
- Limited and grouped routine Dependabot pull requests.

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
