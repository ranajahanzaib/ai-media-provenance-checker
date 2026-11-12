# Security model

AI Media Provenance Checker treats media bytes and pipeline/policy YAML as untrusted. It caps file input at 100 MiB, reads dimensions before full decoding, limits dimensions to 20,000 per side and 100 million pixels, checks crop bounds, restricts step IDs, and rejects unknown pipeline fields and output formats. File outputs are written through a synced temporary file and exclusive hard-link creation, so existing outputs are not overwritten. Input symlinks are rejected.

The C2PA SDK reads embedded JPEG/PNG manifests using the default local context. AI Media Provenance Checker does not resolve arbitrary manifest URLs. Parsing success is not trust; the SDK validation state and trust state are surfaced independently. Reports escape HTML/SVG text and do not serialize signing key material. Run reports contain local paths and should be reviewed before sharing.

Signing uses an explicitly selected SDK settings file or the explicit `--test-signing` switch. Test mode asks the C2PA SDK to generate an ephemeral key in memory; the repository contains no signing keys. Keep production credentials outside the repository and protect them with operating-system controls. AI Media Provenance Checker is a local CLI, not a certificate authority or production signing service.

Known hardening limits include no configurable CPU/time budget, no detached credentials, no exhaustive adversarial C2PA corpus, and no independent `c2patool` verification in the current environment. Do not expose it as a network service without a separate threat review.
