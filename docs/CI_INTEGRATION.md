# CI integration example

For an existing image-processing pipeline, generate an AI Media Provenance Checker report from the same source and step definitions used by the application. Use a protected SDK signing configuration for production signing workflows. The ephemeral test identity demonstrates behavior in a test job; its signatures are expected to be untrusted.

```yaml
name: provenance-check
on: [push, pull_request]
jobs:
  image-provenance:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Build AI Media Provenance Checker
        run: cargo build --locked --release
      - name: Run a deterministic diagnostic fixture
        run: |
          ./target/release/ai-media-provenance-checker fixtures generate --output-dir "$RUNNER_TEMP/fixture" --test-signing
          ./target/release/ai-media-provenance-checker run \
            --input "$RUNNER_TEMP/fixture/sample.png" \
            --pipeline examples/pipelines/full-chain.yml \
            --output-dir "$RUNNER_TEMP/ai-media-provenance-checker-run" \
            --mode compare --test-signing \
            --policy examples/policies/strict.yml
          ./target/release/ai-media-provenance-checker verify \
            --run "$RUNNER_TEMP/ai-media-provenance-checker-run/run.json" \
            --policy examples/policies/strict.yml
          ./target/release/ai-media-provenance-checker report \
            --run "$RUNNER_TEMP/ai-media-provenance-checker-run/run.json" \
            --format html --output "$RUNNER_TEMP/ai-media-provenance-checker-run/report.html"
      - uses: actions/upload-artifact@v4
        if: always()
        with:
          name: ai-media-provenance-checker-report
          path: ${{ runner.temp }}/ai-media-provenance-checker-run
```

To compare an application-produced derivative directly, call `ai-media-provenance-checker compare --source INPUT --derived OUTPUT --policy POLICY` and retain its JSON output and exit status. The bytes for both files must be available in the job. That command evaluates evidence; it does not repair or reissue credentials for arbitrary external processing. Replace the demo fixture and pipeline with your own inputs and operations before relying on the result.
