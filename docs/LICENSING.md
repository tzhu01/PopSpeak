# Licensing and release identity

PopSpeak's original source code uses the MIT License. Its SPDX identifier is
`MIT`, and the license is approved by the Open Source Initiative. The complete,
controlling text is in [`LICENSE`](../LICENSE); this explanatory page does not
add conditions to it.

Under MIT, recipients may use, copy, modify, merge, publish, distribute,
sublicense and sell copies of the covered source, including in commercial
products, provided the required copyright and permission notice is retained.
The license also contains its warranty and liability disclaimer. PopSpeak does
not add a non-commercial clause, activation restriction, anti-fork term or other
field-of-use restriction to the source-code license.

Three boundaries still matter:

1. **Third-party material.** Models, model conversions, native runtimes,
   JavaScript packages and Rust crates retain their respective licenses. Consult
   [`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md) before redistributing a
   binary or model bundle.
2. **Brand and origin.** The MIT grant for code is separate from the PopSpeak
   name, icon and the identity of official signed builds. Forks may truthfully
   describe their origin, but may not misrepresent a third-party build as an
   official PopSpeak release. See [`TRADEMARKS.md`](../TRADEMARKS.md).
3. **Official binaries and services.** Activation, signing, hosted services and
   release-channel policies describe official distributions and services. They
   do not change the MIT rights granted for the covered source code, and a
   self-built fork is not an official signed PopSpeak binary.

The package manifests use the SPDX expression `MIT`:

- `package.json`: `"license": "MIT"`
- `src-tauri/Cargo.toml`: `license = "MIT"`

Saying that MIT is OSI-approved describes the license only. It does not imply
that OSI endorses, certifies or sponsors PopSpeak, and this project does not use
the OSI logo for that claim.
