# Limitations

- AI Media Provenance Checker does not detect AI-generated images, deepfakes, or watermarks.
- C2PA validation does not establish that historical assertions are factually true.
- An unsigned image is not evidence that it is authentic, unmodified, or human-made.
- AI Media Provenance Checker cannot restore historical credentials discarded by another system.
- A valid cryptographic signature does not automatically mean the signer is publicly trusted.
- Compatibility with every editor, CDN, encoder, and manifest flavor is not guaranteed.
- Embedded JPEG and PNG are supported; WebP, sidecars, video, audio, and PDF are not implemented.
- No production certificate issuance or automatic trust-list management is provided.
- The policy implementation covers a small set of required checks and is not a general policy language.
- Standalone graph output is DOT. The HTML report embeds its own SVG graph; Graphviz-based SVG rendering is an optional external step.
- The integration suite includes a readable JPEG with a deliberately altered scan byte; the SDK reports an invalid content binding. It also covers signed JPEG and PNG inspection, wrong and missing parent links, and PNG multi-step ingredient continuity. Malformed-manifest coverage and hosted Linux/macOS runs remain unverified.
- `verify` reopens files, checks their recorded hashes, recomputes parent links, and can re-evaluate policy. It does not recreate the original pipeline execution from scratch.
