use serde_json::Value;
use std::{fs, process::Command};

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ai-media-provenance-checker"))
}

fn generate(output: &std::path::Path, signed: bool) {
    let mut command = cli();
    command
        .args(["fixtures", "generate", "--output-dir"])
        .arg(output);
    if signed {
        command.arg("--test-signing");
    }
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn inspection_distinguishes_unsigned_malformed_and_missing_assets() {
    let temp = tempfile::tempdir().unwrap();
    let fixtures = temp.path().join("fixtures");
    generate(&fixtures, false);

    for name in ["sample.jpg", "sample.png"] {
        let output = cli()
            .arg("inspect")
            .arg(fixtures.join(name))
            .arg("--json")
            .output()
            .unwrap();
        assert!(output.status.success());
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["credentials"], "absent");
        assert_eq!(report["integrity"], "unknown");
    }

    let missing = cli()
        .arg("inspect")
        .arg(temp.path().join("missing.jpg"))
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("reading"));

    let malformed = temp.path().join("malformed.png");
    fs::write(&malformed, b"not an image").unwrap();
    let malformed_result = cli().arg("inspect").arg(malformed).output().unwrap();
    assert!(!malformed_result.status.success());
    assert!(String::from_utf8_lossy(&malformed_result.stderr).contains("image"));

    let unsupported = temp.path().join("unsupported.gif");
    fs::write(
        &unsupported,
        [
            0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0x00,
            0x00, 0x00, 0xff, 0xff, 0xff, 0x21, 0xf9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2c,
            0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00,
            0x3b,
        ],
    )
    .unwrap();
    let unsupported_result = cli().arg("inspect").arg(unsupported).output().unwrap();
    assert!(!unsupported_result.status.success());
    assert!(String::from_utf8_lossy(&unsupported_result.stderr).contains("unsupported"));
}

#[test]
fn jpeg_png_conversion_chain_produces_readable_outputs() {
    let temp = tempfile::tempdir().unwrap();
    let fixtures = temp.path().join("fixtures");
    generate(&fixtures, false);
    let pipeline = temp.path().join("formats.yml");
    fs::write(
        &pipeline,
        "version: 1\nsteps:\n  - id: jpeg-to-png\n    type: convert\n    format: png\n  - id: png-to-jpeg\n    type: convert\n    format: jpeg\n",
    )
    .unwrap();
    let output_dir = temp.path().join("run");
    let result = cli()
        .args(["run", "--input"])
        .arg(fixtures.join("sample.jpg"))
        .args(["--pipeline"])
        .arg(pipeline)
        .args(["--output-dir"])
        .arg(&output_dir)
        .args(["--mode", "raw"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        image::guess_format(&fs::read(output_dir.join("01-jpeg-to-png.png")).unwrap()).unwrap(),
        image::ImageFormat::Png
    );
    assert_eq!(
        image::guess_format(&fs::read(output_dir.join("02-png-to-jpeg.jpg")).unwrap()).unwrap(),
        image::ImageFormat::Jpeg
    );
}

#[test]
fn passthrough_is_byte_identical_and_retains_signed_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let fixtures = temp.path().join("fixtures");
    generate(&fixtures, true);
    let source = fixtures.join("sample.png");
    let original = fs::read(&source).unwrap();
    let pipeline = temp.path().join("copy.yml");
    fs::write(
        &pipeline,
        "version: 1\nname: positive-control\nsteps:\n  - id: copy\n    type: passthrough\n",
    )
    .unwrap();
    let output_dir = temp.path().join("run");
    let run = cli()
        .args(["run", "--input"])
        .arg(&source)
        .args(["--pipeline"])
        .arg(&pipeline)
        .args(["--output-dir"])
        .arg(&output_dir)
        .args(["--mode", "raw"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let copy = output_dir.join("01-copy.png");
    assert_eq!(fs::read(&copy).unwrap(), original);
    let report: Value =
        serde_json::from_slice(&fs::read(output_dir.join("run.json")).unwrap()).unwrap();
    assert_eq!(report["artifacts"][0]["parent_link"], "preserved");
    assert_eq!(
        report["artifacts"][0]["inspection"]["credentials"],
        "present"
    );
    assert_eq!(report["artifacts"][0]["inspection"]["integrity"], "valid");
    assert_eq!(fs::read(&source).unwrap(), original);
}

#[test]
fn doctor_and_help_are_usable_without_signing_secrets() {
    let help = cli().arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Preserve provenance"));

    let doctor = cli().arg("doctor").output().unwrap();
    assert!(doctor.status.success());
    let output = String::from_utf8_lossy(&doctor.stdout);
    assert!(output.contains("C2PA SDK: 0.91.7"));
    assert!(output.contains("Image codecs: JPEG, PNG"));
    assert!(!output.contains("PRIVATE KEY"));
}

#[test]
fn jpeg_encoding_flattens_transparency_onto_the_requested_background() {
    let temp = tempfile::tempdir().unwrap();
    let transparent = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        8,
        8,
        image::Rgba([255, 0, 0, 0]),
    ));
    let source = temp.path().join("transparent.png");
    transparent.save(&source).unwrap();
    let pipeline = temp.path().join("convert.yml");
    fs::write(
        &pipeline,
        "version: 1\nsteps:\n  - id: flatten\n    type: convert\n    format: jpeg\n    background: '#00ff00'\n",
    )
    .unwrap();
    let output_dir = temp.path().join("run");
    let result = cli()
        .args(["run", "--input"])
        .arg(&source)
        .args(["--pipeline"])
        .arg(pipeline)
        .args(["--output-dir"])
        .arg(&output_dir)
        .args(["--mode", "raw"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let decoded = image::open(output_dir.join("01-flatten.jpg"))
        .unwrap()
        .to_rgb8();
    let pixel = decoded.get_pixel(4, 4).0;
    assert!(pixel[0] < 20, "red component was {}", pixel[0]);
    assert!(pixel[1] > 235, "green component was {}", pixel[1]);
    assert!(pixel[2] < 20, "blue component was {}", pixel[2]);
}
