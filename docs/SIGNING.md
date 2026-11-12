# Signing

AI Media Provenance Checker creates a new claim for a transformed output; it does not alter an earlier signed claim or reproduce the earlier signer’s signature. For non-passthrough stages it records the immediate input as a `parentOf` ingredient, records the applicable transformation action, embeds the new manifest, and reopens the output for validation.

Use `--signing-config PATH` with an SDK-compatible settings file whose key and certificate are stored outside the repository and protected by the operating system. `examples/signing/settings.example.toml` shows the expected settings shape but is intentionally not a usable signer. Keep actual credentials in a protected file; do not place them in logs, run artifacts, or version control. Signing failures stop the run.

`--test-signing` asks the official SDK to generate an ephemeral signer in memory for the current process. Its certificate is untrusted by design, and the generated identity must not be used for production claims. No signing key is stored in the repository or written to a fixture file. This option exists for local demos and CI only. `fixtures generate --test-signing` creates development-signed JPEG and PNG fixtures; the default fixture command is unsigned.

The new output claim attributes processing to the configured signer. It cannot establish the historical origin of an unsigned source, and a valid signature does not prove assertions truthful. Trust is reported separately from content-binding validity. The SDK signing and ingredient APIs follow the current context-based Rust SDK. No production certificate issuance or automatic trust-list management is included.
