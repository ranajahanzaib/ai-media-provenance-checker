use serde_json::Value;
use std::{fs, process::Command};

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ai-media-provenance-checker"))
}

#[test]
fn signed_compare_run_revalidates_and_reports_real_c2pa_chain() {
    let temp = tempfile::tempdir().unwrap();
    let fixtures = temp.path().join("fixtures");
    let generated = cli()
        .args(["fixtures", "generate", "--output-dir"])
        .arg(&fixtures)
        .arg("--test-signing")
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let source = fixtures.join("sample.png");
    let inspected = cli()
        .arg("inspect")
        .arg(&source)
        .arg("--json")
        .output()
        .unwrap();
    assert!(inspected.status.success());
    let value: Value = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(value["credentials"], "present");
    assert_eq!(value["integrity"], "valid");
    assert_eq!(value["trust"], "untrusted");
    assert_eq!(value["claim_generator"], "AIMediaProvenanceChecker 0.1.0");

    let jpeg = cli()
        .arg("inspect")
        .arg(fixtures.join("sample.jpg"))
        .arg("--json")
        .output()
        .unwrap();
    assert!(jpeg.status.success());
    let jpeg_value: Value = serde_json::from_slice(&jpeg.stdout).unwrap();
    assert_eq!(jpeg_value["credentials"], "present");
    assert_eq!(jpeg_value["integrity"], "valid");

    let tampered = cli()
        .arg("inspect")
        .arg(fixtures.join("tampered-binding.jpg"))
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        tampered.status.success(),
        "tampered fixture must remain a readable image: {}",
        String::from_utf8_lossy(&tampered.stderr)
    );
    let tampered_value: Value = serde_json::from_slice(&tampered.stdout).unwrap();
    assert_eq!(tampered_value["credentials"], "present");
    assert_eq!(tampered_value["integrity"], "invalid");
    assert!(
        tampered_value["validation"]["validation_status"]
            .as_array()
            .unwrap()
            .iter()
            .any(|status| status["code"] == "assertion.dataHash.mismatch")
    );

    let output_dir = temp.path().join("run");
    let pipeline = format!(
        "{}/examples/pipelines/full-chain.yml",
        env!("CARGO_MANIFEST_DIR")
    );
    let run = cli()
        .args(["run", "--input"])
        .arg(&source)
        .args(["--pipeline", &pipeline, "--output-dir"])
        .arg(&output_dir)
        .args(["--mode", "compare", "--test-signing", "--policy"])
        .arg(format!(
            "{}/examples/policies/strict.yml",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let summary = String::from_utf8_lossy(&run.stdout);
    assert!(summary.contains("Expected raw credential losses: 4"));
    assert!(summary.contains("Policy: passed"));
    let report_path = output_dir.join("run.json");
    let report: Value = serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
    assert_eq!(report["schema_version"], "1.0");
    assert_eq!(report["policy"]["passed"], true);
    let records = report["artifacts"].as_array().unwrap();
    assert!(
        records
            .iter()
            .any(|a| a["branch"] == "raw" && a["inspection"]["credentials"] == "absent")
    );
    let reissued: Vec<_> = records
        .iter()
        .filter(|a| a["branch"] == "reissued")
        .collect();
    assert_eq!(reissued.len(), 4);
    assert!(
        reissued
            .iter()
            .all(|a| a["inspection"]["integrity"] == "valid")
    );
    assert!(reissued.iter().all(|a| a["parent_link"] == "linked"));

    let raw_dir = temp.path().join("raw-policy");
    let raw = cli()
        .args(["run", "--input"])
        .arg(&source)
        .args(["--pipeline", &pipeline, "--output-dir"])
        .arg(&raw_dir)
        .args(["--mode", "raw", "--policy"])
        .arg(format!(
            "{}/examples/policies/strict.yml",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        !raw.status.success(),
        "strict policy should reject raw losses"
    );
    let raw_report: Value =
        serde_json::from_slice(&fs::read(raw_dir.join("run.json")).unwrap()).unwrap();
    assert_eq!(raw_report["policy"]["passed"], false);
    assert!(
        raw_report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["code"] == "POLICY_VIOLATION")
    );

    let diagnostic = cli()
        .args(["run", "--input"])
        .arg(&source)
        .args(["--pipeline", &pipeline, "--output-dir"])
        .arg(temp.path().join("diagnostic"))
        .args(["--mode", "raw", "--policy"])
        .arg(format!(
            "{}/examples/policies/diagnostic.yml",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        diagnostic.status.success(),
        "{}",
        String::from_utf8_lossy(&diagnostic.stderr)
    );

    let other_fixtures = temp.path().join("other-fixtures");
    let other_generated = cli()
        .args(["fixtures", "generate", "--output-dir"])
        .arg(&other_fixtures)
        .arg("--test-signing")
        .output()
        .unwrap();
    assert!(other_generated.status.success());
    let wrong_parent_run = temp.path().join("wrong-parent-run");
    let reissued_from_other = cli()
        .args(["run", "--input"])
        .arg(other_fixtures.join("sample.png"))
        .args(["--pipeline", &pipeline, "--output-dir"])
        .arg(&wrong_parent_run)
        .args(["--mode", "reissue", "--test-signing"])
        .output()
        .unwrap();
    assert!(
        reissued_from_other.status.success(),
        "{}",
        String::from_utf8_lossy(&reissued_from_other.stderr)
    );
    let mismatched = cli()
        .args(["compare", "--source"])
        .arg(&source)
        .args(["--derived"])
        .arg(wrong_parent_run.join("reissued/01-resized.png"))
        .output()
        .unwrap();
    assert!(mismatched.status.success());
    let mismatch_report: Value = serde_json::from_slice(&mismatched.stdout).unwrap();
    assert_eq!(mismatch_report["parent_link"], "wrong_parent");
    assert!(
        mismatch_report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["code"] == "PARENT_LINK_MISMATCH")
    );

    let missing_parent = cli()
        .args(["compare", "--source"])
        .arg(&source)
        .args(["--derived"])
        .arg(output_dir.join("raw/01-resized.png"))
        .output()
        .unwrap();
    assert!(missing_parent.status.success());
    let missing_report: Value = serde_json::from_slice(&missing_parent.stdout).unwrap();
    assert_eq!(missing_report["parent_link"], "missing_parent");
    assert!(
        missing_report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["code"] == "PARENT_LINK_MISSING")
    );

    let trust_policy = temp.path().join("trusted.yml");
    fs::write(
        &trust_policy,
        "version: 1\nrequirements:\n  require_trusted_signer: true\n",
    )
    .unwrap();
    let untrusted = cli()
        .args(["compare", "--source"])
        .arg(&source)
        .args(["--derived"])
        .arg(wrong_parent_run.join("reissued/01-resized.png"))
        .args(["--policy"])
        .arg(trust_policy)
        .output()
        .unwrap();
    assert!(!untrusted.status.success());
    assert!(String::from_utf8_lossy(&untrusted.stderr).contains("signer trust is untrusted"));

    let verified = cli()
        .args(["verify", "--run"])
        .arg(&report_path)
        .args([
            "--policy",
            &format!(
                "{}/examples/policies/strict.yml",
                env!("CARGO_MANIFEST_DIR")
            ),
        ])
        .output()
        .unwrap();
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
    let html = temp.path().join("report.html");
    let reported = cli()
        .args(["report", "--run"])
        .arg(&report_path)
        .args(["--format", "html", "--output"])
        .arg(&html)
        .output()
        .unwrap();
    assert!(reported.status.success());
    let html = fs::read_to_string(html).unwrap();
    assert!(html.contains("Processing stages"));
    assert!(html.contains("Provenance graph"));
    assert!(html.contains("<svg"));
    assert!(!html.contains("<script"));
    assert!(!html.contains("PRIVATE KEY"));

    let dot_path = temp.path().join("chain.dot");
    let dot = cli()
        .args(["report", "--run"])
        .arg(&report_path)
        .args(["--format", "dot", "--output"])
        .arg(&dot_path)
        .output()
        .unwrap();
    assert!(dot.status.success());
    let dot = fs::read_to_string(dot_path).unwrap();
    assert!(dot.contains("source -> artifact_0"));
    assert!(dot.contains("artifact_6 -> artifact_7"));
    assert!(dot.contains("style=solid"));
    assert!(dot.contains("style=dashed"));

    let json_path = temp.path().join("report.json");
    let json = cli()
        .args(["report", "--run"])
        .arg(&report_path)
        .args(["--format", "json", "--output"])
        .arg(&json_path)
        .output()
        .unwrap();
    assert!(json.status.success());
    let exported: Value = serde_json::from_slice(&fs::read(json_path).unwrap()).unwrap();
    assert_eq!(exported["schema_version"], "1.0");

    let first_output = output_dir.join("reissued/01-resized.png");
    let mut tampered_output = fs::read(&first_output).unwrap();
    tampered_output[100] ^= 1;
    fs::write(first_output, tampered_output).unwrap();
    let stale = cli()
        .args(["verify", "--run"])
        .arg(&report_path)
        .output()
        .unwrap();
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("artifact hash changed"));
}
