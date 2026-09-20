# Runtime supply-chain integrity

PopSpeak treats native executables, their DLL dependencies and native build
sources as one authenticated unit. The reviewed values live in
`scripts/runtime-integrity-manifest.json`; changing a release, commit, file set,
size or SHA-256 is a reviewable source change rather than an automatic update.

## Pinned inputs

| Input                                         | Immutable version                           |      Bytes | SHA-256                                                            |
| --------------------------------------------- | ------------------------------------------- | ---------: | ------------------------------------------------------------------ |
| FunASR GitHub codeload ZIP                    | `fcf9a0d4a604d0859fc5927cca04df141e3b09c8`  | 61,464,364 | `2c7f02c23c40a26df1d46ac589f820031883ce071d36c3feda92b44e354eb817` |
| llama.cpp source ZIP used by the FunASR build | `803b7fcae893e9caaee3921779628fef83ac0965`  | 38,453,063 | `8431e10c4df5877dfc9a0fb6ffe1e88bf4f1f96d93b5d112e90556281584452a` |
| whisper.cpp Windows BLAS ZIP                  | `v1.7.6` / `whisper-blas-bin-x64.zip`       | 16,242,682 | `adde1afb6e915ae522fffeef117ca178e2561d39487c164f1a7c3899d41b4a1d` |
| llama.cpp Windows CPU ZIP                     | `b9959` / `llama-b9959-bin-win-cpu-x64.zip` | 18,210,062 | `e7b44f74a8413b96fc79551cebae517d1f5371ca4aec28d40d0a5589db0783b0` |

The two release-asset hashes above were checked against the SHA-256 digests
published by the GitHub Releases API. The source ZIPs use official GitHub
codeload URLs for exact commits and are byte-pinned. GitHub may theoretically
re-encode an automatically generated source archive in the future; that will
intentionally stop the build. Review the new archive and update the manifest—do
not bypass the check.

## Enforcement

- `build-funasr-pipe-host.ps1` accepts only the reviewed FunASR revision. A
  caller-supplied `UpstreamSource` is rejected because an arbitrary directory
  cannot be authenticated by the pinned archive digest.
- A cached source archive is checked for exact size and SHA-256 before every
  use. Expanded source directories are recreated from the verified archive on
  every build; a revision marker is not treated as proof of integrity.
- The llama.cpp revision parsed from FunASR's CMake file must exactly match the
  reviewed dependency revision and archive digest.
- `fetch-whisper.ps1` and `fetch-llama-server.ps1` reuse an installed runtime
  only if its complete expected file set, sizes and per-file SHA-256 values
  match. Missing, modified, and unexpected DLLs all cause rejection.
- Downloaded release ZIPs and every selected file are verified in a staging
  directory before the destination runtime is replaced. Only the explicitly
  reviewed subset is installed; unrelated tools from the release archive are
  not copied.

Run the offline manifest and positive/negative fixture checks with:

```powershell
./scripts/test-runtime-integrity.ps1
```

This integrity work does **not** clear the separate binary redistribution
blocker. The pinned llama.cpp asset contains `libomp140.x86_64.dll`; public
binary packaging must continue to fail in
`scripts/assert-redistributable-runtime.ps1` until PopSpeak uses a legally
redistributable, independently built and audited alternative. See
`docs/OPEN_SOURCE_BLOCKERS.md`.
