# PopSpeak Privacy Notes

PopSpeak is local-first. This document describes the behavior of the source in
this repository; a distributor that changes the application or adds a service
must update this notice.

## Local recognition

The default SenseVoice, optional Fun-ASR/Whisper/native-ASR engines, the live
approximate preview lane, vocabulary, corrections and local text polishing run
on the user's computer. The preview lane receives the same 16 kHz mono PCM as
the selected final recognizer and is never used as the authoritative result.
PopSpeak does not intentionally save raw microphone audio.

## Optional network features

- Selecting a cloud speech provider sends microphone audio and the configured
  recognition options to that provider.
- Selecting a cloud LLM sends the recognized text, enabled selected-text
  context and relevant vocabulary/context to that provider.
- Model download, update check, public-account activation and optional account
  features connect to the endpoints named in the UI or build configuration.

These features are optional. Provider terms, retention and billing are governed
by the selected third party. A locally activated license does not include cloud
quota.

## Data kept on the computer

PopSpeak stores settings, recognition history, dictionary/correction entries,
reward state and a short crash-recovery transcript journal in the Tauri app-data
directory for identifier `com.popspeak.desktop`. History records include the
recognized text, target application name, language and recording duration.
Window titles are used transiently to avoid typing into the wrong window and are
not written to history.

When selected-text context is enabled, PopSpeak temporarily reads the clipboard,
captures the selection, and restores the previous clipboard content. The
selection is used for that request and is not added as a separate history row.

On Windows, the generic STT and LLM API-key fields use Windows Credential
Manager. The dedicated SeedASR credential and custom-vendor credential fields
are currently stored in plaintext in the local `settings.json`; do not share
that file. Session login tokens remain in process memory.

## Logs and support bundles

Diagnostic output is intended to contain provider names, timing, byte/character
counts and errors rather than dictated text or credentials. File paths and
service error identifiers may still be sensitive. Review and redact logs before
sharing them publicly.

## User controls

Individual or all history rows can be deleted in the app. Dictionary entries and
settings can be edited in Settings. To remove all local state, exit PopSpeak and
delete its `com.popspeak.desktop` app-data directory and the PopSpeak entries in
Windows Credential Manager. Deleting local data does not delete data already
sent to an optional third-party cloud provider.

Security issues should be reported through the private channel in
[SECURITY.md](SECURITY.md).
