# Pipelines

Pipeline YAML has `version: 1`, an optional display `name`, optional `input.expected_format` (`jpeg` or `png`), and an ordered `steps` list. IDs must be unique and use ASCII letters, digits, `_`, or `-`. No shell commands or expressions are evaluated.

Supported steps:

```yaml
steps:
  - id: resize
    type: resize
    width: 1200
    height: 800
    sizing: fit # or exact (default)
    filter: lanczos3
    format: jpeg # optional; defaults to source format
  - id: crop
    type: crop
    x: 0
    y: 0
    width: 800
    height: 600
    format: jpeg # optional
  - id: compress
    type: compress
    quality: 80
  - id: convert
    type: convert
    format: png
  - id: unchanged
    type: passthrough
```

Resize supports nearest, triangle, catmullrom, or lanczos3. `fit` keeps aspect ratio within the requested box; `exact` forces the requested dimensions. Crops outside image bounds fail. JPEG quality is 1–100, default 85. When alpha is flattened into JPEG, background defaults to `#ffffff`; a step may set `background: "#RRGGBB"`. Outputs must be JPEG or PNG. Source images are limited to 100 MiB and 100 million decoded pixels. Each run output path is exclusive; choose a fresh directory for each run.

Examples are in `examples/pipelines/`. Run with `--mode raw`, `--mode reissue`, or `--mode compare`; reissue requires `--signing-config` or explicit `--test-signing`.
