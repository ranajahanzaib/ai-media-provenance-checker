# Reports

`run.json` is the versioned run record (`schema_version: "1.0"`). Its JSON Schema is [run.schema.json](../schemas/run.schema.json). It includes the pipeline name and mode, source inspection, ordered artifact records, operation parameters, input/output sizes and hashes, inspections, parent-link assessments, findings, and optional policy result. Unknown or unavailable evidence is represented explicitly. Paths are local execution paths and may disclose directory names; review before sharing reports.

Generate report formats with:

```sh
ai-media-provenance-checker report --run run.json --format json --output report.json
ai-media-provenance-checker report --run run.json --format html --output report.html
ai-media-provenance-checker report --run run.json --format dot --output chain.dot
```

HTML is self-contained and offline. Dynamic text is HTML escaped; reports contain no signing key material. DOT represents recorded branch sequences and parent-link status. Graphviz SVG rendering is not built in; use `dot -Tsvg chain.dot -o chain.svg` if Graphviz is installed. The report is diagnostic and should be interpreted with the state definitions in `VALIDATION.md`.
