import os
import subprocess

html_template = """<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    background: #0f141c;
    padding: 30px;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
    display: flex;
    justify-content: center;
  }
  .window {
    width: 980px;
    background: #0d1117;
    border-radius: 12px;
    box-shadow: 0 20px 50px rgba(0,0,0,0.5), 0 0 0 1px rgba(255,255,255,0.1);
    overflow: hidden;
  }
  .titlebar {
    background: #161b22;
    padding: 12px 16px;
    display: flex;
    align-items: center;
    border-bottom: 1px solid #30363d;
  }
  .buttons {
    display: flex;
    gap: 8px;
  }
  .btn {
    width: 12px;
    height: 12px;
    border-radius: 50%;
  }
  .btn-close { background: #ff5f56; }
  .btn-min { background: #ffbd2e; }
  .btn-max { background: #27c93f; }
  .title {
    flex-grow: 1;
    text-align: center;
    color: #8b949e;
    font-size: 13px;
    font-weight: 500;
    margin-right: 48px;
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
  }
  .terminal {
    padding: 24px;
    font-family: "SF Mono", "Menlo", "Monaco", "Consolas", monospace;
    font-size: 13.5px;
    line-height: 1.55;
    color: #c9d1d9;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .prompt { color: #58a6ff; font-weight: 600; }
  .cmd { color: #f0f6fc; font-weight: 600; }
  .flag { color: #79c0ff; }
  .comment { color: #8b949e; font-style: italic; }
  .ok { color: #3fb950; font-weight: 600; }
  .warn { color: #d29922; }
  .err { color: #f85149; }
  .table-header { color: #f0f6fc; font-weight: 700; }
  .table-line { color: #30363d; }
  .badge-untrusted { color: #d29922; }
  .badge-missing { color: #f85149; }
  .badge-linked { color: #3fb950; }
  .key { color: #8b949e; }
  .val { color: #58a6ff; }
  .bold { font-weight: 700; color: #f0f6fc; }
</style>
</head>
<body>
  <div class="window">
    <div class="titlebar">
      <div class="buttons">
        <div class="btn btn-close"></div>
        <div class="btn btn-min"></div>
        <div class="btn btn-max"></div>
      </div>
      <div class="title">{title}</div>
    </div>
    <div class="terminal">{content}</div>
  </div>
</body>
</html>
"""

# 1. Pipeline Run Terminal Content
pipeline_content = """<span class="prompt">$</span> <span class="cmd">ai-media-provenance-checker run</span> \\
    <span class="flag">--input</span> photo.png \\
    <span class="flag">--pipeline</span> examples/pipelines/full-chain.yml \\
    <span class="flag">--output-dir</span> ./artifacts/demo/run \\
    <span class="flag">--mode</span> compare \\
    <span class="flag">--test-signing</span> \\
    <span class="flag">--policy</span> examples/policies/strict.yml

<span class="bold">AI Media Provenance Checker — Pipeline Analysis</span>

Pipeline: website-image-processing
Mode: compare
Source credentials: <span class="ok">present</span> (<span class="ok">valid</span>)

<span class="table-header">Step                 Raw                        Reissued</span>
<span class="table-line">──────────────────────────────────────────────────────────────────────────────</span>
resized              <span class="badge-missing">Missing</span>                    <span class="ok">Valid</span> <span class="badge-untrusted">(untrusted)</span> + <span class="badge-linked">linked</span>
thumbnail            <span class="badge-missing">Missing</span>                    <span class="ok">Valid</span> <span class="badge-untrusted">(untrusted)</span> + <span class="badge-linked">linked</span>
social-compress      <span class="badge-missing">Missing</span>                    <span class="ok">Valid</span> <span class="badge-untrusted">(untrusted)</span> + <span class="badge-linked">linked</span>
png-derivative       <span class="badge-missing">Missing</span>                    <span class="ok">Valid</span> <span class="badge-untrusted">(untrusted)</span> + <span class="badge-linked">linked</span>

Findings: 16 · Expected raw credential losses: 4 · Verified parent links: 4
Run record: ./artifacts/demo/run/run.json
Policy: <span class="ok">passed</span>

<span class="prompt">$</span> <span class="cmd">ai-media-provenance-checker verify</span> \\
    <span class="flag">--run</span> ./artifacts/demo/run/run.json \\
    <span class="flag">--policy</span> examples/policies/strict.yml

<span class="ok">OK</span>  artifacts/demo/run/raw/01-resized.png
<span class="ok">OK</span>  artifacts/demo/run/raw/02-thumbnail.jpg
<span class="ok">OK</span>  artifacts/demo/run/raw/03-social-compress.jpg
<span class="ok">OK</span>  artifacts/demo/run/raw/04-png-derivative.png
<span class="ok">OK</span>  artifacts/demo/run/reissued/01-resized.png
<span class="ok">OK</span>  artifacts/demo/run/reissued/02-thumbnail.jpg
<span class="ok">OK</span>  artifacts/demo/run/reissued/03-social-compress.jpg
<span class="ok">OK</span>  artifacts/demo/run/reissued/04-png-derivative.png
<span class="bold">Verified source and 8 output artifacts.</span>"""

# 2. Inspect Valid vs Tampered Content
inspect_content = """<span class="comment"># 1. Inspect authentic signed asset</span>
<span class="prompt">$</span> <span class="cmd">ai-media-provenance-checker inspect</span> artifacts/demo/fixtures/sample.png

<span class="bold">AI Media Provenance Checker Inspection</span>

Asset: sample.png
Format: Png · 640 × 480 · 349,525 bytes
SHA-256: e25ab032dda653359152e18896ade779a112605fec3b8c128b4841e221ac654c

Content Credentials: <span class="ok">present</span>
Integrity:           <span class="ok">valid</span>
Trust:               <span class="warn">untrusted</span>
Active manifest:     urn:c2pa:1a85be82-3b3b-407a-ba55-e57a84975102
Claim generator:     AIMediaProvenanceChecker 0.1.0
Ingredients:         0
Actions:             c2pa.created

<span class="comment"># 2. Inspect tampered asset: content was modified after signing</span>
<span class="prompt">$</span> <span class="cmd">ai-media-provenance-checker inspect</span> artifacts/demo/fixtures/tampered-binding.jpg

<span class="bold">AI Media Provenance Checker Inspection</span>

Asset: tampered-binding.jpg
Format: Jpeg · 640 × 480 · 39,697 bytes
SHA-256: 524a85d77f76df826a507cf2afb9448518237b22dcfcc6eee0730476dc5d4d75

Content Credentials: <span class="ok">present</span>
Integrity:           <span class="err">invalid (content-binding mismatch)</span>
Trust:               unknown
Active manifest:     urn:c2pa:cc9c95fb-fc5c-4289-a856-9a2a404aee14
Claim generator:     AIMediaProvenanceChecker 0.1.0
Validation failure:  <span class="err">assertion.dataHash.mismatch</span>"""

# 3. Doctor & Verification
doctor_content = """<span class="prompt">$</span> <span class="cmd">ai-media-provenance-checker doctor</span>

AI Media Provenance Checker 0.1.0
Rust: rustc 1.99.1
C2PA SDK: 0.92.1
Image codecs: JPEG, PNG
Signing config: not supplied (use --signing-config for local signing)
Test signing: explicit --test-signing only
Graphviz: available (DOT export can be rendered externally)

<span class="prompt">$</span> <span class="cmd">ai-media-provenance-checker report</span> \\
    <span class="flag">--run</span> ./artifacts/demo/run/run.json \\
    <span class="flag">--format</span> html \\
    <span class="flag">--output</span> ./artifacts/demo/report.html

<span class="ok">Wrote HTML report:</span> ./artifacts/demo/report.html
<span class="comment">Interactive report includes embedded SVG provenance graph, step comparison table, and findings.</span>"""

os.makedirs("target/screenshot-html", exist_ok=True)

with open("target/screenshot-html/pipeline.html", "w") as f:
    f.write(html_template.replace("{title}", "ai-media-provenance-checker run --mode compare").replace("{content}", pipeline_content))

with open("target/screenshot-html/inspect.html", "w") as f:
    f.write(html_template.replace("{title}", "ai-media-provenance-checker inspect (tamper detection)").replace("{content}", inspect_content))

with open("target/screenshot-html/doctor.html", "w") as f:
    f.write(html_template.replace("{title}", "ai-media-provenance-checker doctor & reporting").replace("{content}", doctor_content))

print("HTML mockups generated.")
