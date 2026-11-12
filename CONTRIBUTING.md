# Contributing

Contributions should preserve accurate C2PA semantics. Keep credential presence, integrity, trust, and chain continuity separate. Never use a test identity in production-oriented commands, and do not claim that a valid signature proves the truth of its assertions.

## Development

Use stable Rust 1.96 or newer. Before opening a pull request, run:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
```

New C2PA behavior should use the official SDK's current explicit `Context`, `Reader`, and `Builder` APIs. Add real signed media integration coverage for behavior that depends on signing or validation; do not substitute string-only checks for SDK validation. Do not commit production credentials or private keys.

Please include a concise problem statement, implementation notes, and the commands actually run. See the [Code of Conduct](CODE_OF_CONDUCT.md) and [Security Policy](SECURITY.md).
