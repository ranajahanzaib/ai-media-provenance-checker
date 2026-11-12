use anyhow::{Context, Result, bail};
use c2pa::{Builder, BuilderIntent, Context as C2paContext, Settings};
use clap::{Parser, Subcommand, ValueEnum};
use image::{DynamicImage, ImageFormat, imageops::FilterType};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Cursor,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(
    name = "ai-media-provenance-checker",
    version,
    about = "Preserve provenance through image-processing pipelines"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Inspect {
        image: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Fixtures {
        #[command(subcommand)]
        command: FixturesCommand,
    },
    Run {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        pipeline: PathBuf,
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long, value_enum, default_value = "raw")]
        mode: Mode,
        #[arg(
            long,
            conflicts_with = "signing_config",
            help = "Generate an ephemeral, untrusted signer in memory for testing"
        )]
        test_signing: bool,
        #[arg(long)]
        signing_config: Option<PathBuf>,
        #[arg(long)]
        policy: Option<PathBuf>,
    },
    Compare {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        derived: PathBuf,
        #[arg(long)]
        policy: Option<PathBuf>,
    },
    Verify {
        #[arg(long)]
        run: PathBuf,
        #[arg(long)]
        policy: Option<PathBuf>,
    },
    Report {
        #[arg(long)]
        run: PathBuf,
        #[arg(long, value_enum)]
        format: ReportFormat,
        #[arg(long)]
        output: PathBuf,
    },
    Doctor {
        #[arg(long)]
        signing_config: Option<PathBuf>,
    },
}
#[derive(Subcommand)]
enum FixturesCommand {
    Generate {
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        test_signing: bool,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    Raw,
    Reissue,
    Compare,
}
#[derive(Clone, Copy, ValueEnum)]
enum ReportFormat {
    Json,
    Html,
    Dot,
}
#[derive(Serialize, Deserialize, Clone)]
struct Inspection {
    path: String,
    format: String,
    width: u32,
    height: u32,
    bytes: u64,
    sha256: String,
    credentials: String,
    integrity: String,
    trust: String,
    active_manifest: Option<String>,
    manifest_id: Option<String>,
    claim_generator: Option<String>,
    actions: Vec<String>,
    ingredients: usize,
    ingredient_evidence: Vec<IngredientEvidence>,
    validation: serde_json::Value,
}
#[derive(Serialize, Deserialize, Clone)]
struct IngredientEvidence {
    title: Option<String>,
    relationship: String,
    active_manifest: Option<String>,
    validation: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct RunReport {
    schema_version: String,
    mode: String,
    pipeline_name: String,
    source: Inspection,
    artifacts: Vec<ArtifactRecord>,
    findings: Vec<Finding>,
    policy: Option<PolicyResult>,
}
#[derive(Serialize, Deserialize)]
struct ArtifactRecord {
    branch: String,
    step_id: String,
    operation: String,
    input_sha256: String,
    transform: TransformInfo,
    parent_link: String,
    inspection: Inspection,
}
#[derive(Serialize, Deserialize, Clone)]
struct TransformInfo {
    parameters: serde_json::Value,
    input_bytes: u64,
    output_bytes: u64,
}
#[derive(Serialize, Deserialize, Clone)]
struct Finding {
    code: String,
    severity: String,
    message: String,
    artifact: Option<String>,
    expected: Option<String>,
    actual: Option<String>,
    remediation: Option<String>,
    expected_negative: bool,
}
#[derive(Serialize, Deserialize, Clone)]
struct PolicyResult {
    passed: bool,
    violations: Vec<String>,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct PolicyFile {
    version: Option<u32>,
    requirements: PolicyRequirements,
    behavior: PolicyBehavior,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct PolicyRequirements {
    require_output_credentials: bool,
    require_valid_integrity: bool,
    require_parent_link: bool,
    require_transformation_action: bool,
    require_trusted_signer: bool,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct PolicyBehavior {
    fail_on_unverifiable: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pipeline {
    #[serde(default = "default_pipeline_version")]
    version: u32,
    #[serde(default = "default_pipeline_name")]
    name: String,
    #[serde(default)]
    input: PipelineInput,
    steps: Vec<Step>,
}
fn default_pipeline_version() -> u32 {
    1
}
fn default_pipeline_name() -> String {
    "image-pipeline".into()
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct PipelineInput {
    expected_format: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    width: Option<u32>,
    height: Option<u32>,
    x: Option<u32>,
    y: Option<u32>,
    filter: Option<String>,
    quality: Option<u8>,
    format: Option<String>,
    sizing: Option<String>,
    background: Option<String>,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Inspect { image, json } => {
            let report = inspect(&image)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_inspection(&report);
            }
        }
        Command::Fixtures {
            command:
                FixturesCommand::Generate {
                    output_dir,
                    test_signing,
                },
        } => {
            fs::create_dir_all(&output_dir)?;
            let signing = test_signing
                .then(|| signing_context(None, true))
                .transpose()?;
            for (extension, mime) in [("png", "image/png"), ("jpg", "image/jpeg")] {
                let mut bytes = encode_image(&fixture(), extension, None, None)?;
                if let Some(ctx) = &signing {
                    bytes = sign_created(ctx, &bytes, mime)?;
                }
                let path = output_dir.join(format!("sample.{extension}"));
                atomic_write_new(&path, &bytes)?;
                println!(
                    "Generated {} deterministic fixture: {}",
                    if test_signing {
                        "TEST-SIGNED"
                    } else {
                        "unsigned"
                    },
                    path.display()
                );
            }
            if let Some(ctx) = &signing {
                let mut bytes = encode_image(&fixture(), "jpeg", None, None)?;
                bytes = sign_created(ctx, &bytes, "image/jpeg")?;
                tamper_jpeg_content_binding(&mut bytes)?;
                let path = output_dir.join("tampered-binding.jpg");
                atomic_write_new(&path, &bytes)?;
                println!("Generated invalid-binding test fixture: {}", path.display());
            }
        }
        Command::Run {
            input,
            pipeline,
            output_dir,
            mode,
            test_signing,
            signing_config,
            policy,
        } => run(
            &input,
            &pipeline,
            &output_dir,
            mode,
            test_signing,
            signing_config.as_deref(),
            policy.as_deref(),
        )?,
        Command::Compare {
            source,
            derived,
            policy,
        } => compare(&source, &derived, policy.as_deref())?,
        Command::Verify { run, policy } => verify(&run, policy.as_deref())?,
        Command::Report {
            run,
            format,
            output,
        } => report(&run, format, &output)?,
        Command::Doctor { signing_config } => {
            let rustc = std::process::Command::new("rustc")
                .arg("--version")
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
                .unwrap_or_else(|| "not found".into());
            let signing_status = signing_config.as_ref().map_or_else(
                || "not supplied (use --signing-config for local signing)".to_owned(),
                |path| match Settings::new()
                    .with_file(path)
                    .and_then(|settings| C2paContext::new().with_settings(settings).map(|_| ()))
                {
                    Ok(()) => format!("valid SDK settings: {}", path.display()),
                    Err(error) => format!("invalid/unreadable: {} ({error})", path.display()),
                },
            );
            println!(
                "AI Media Provenance Checker {}\nRust: {rustc}\nC2PA SDK: 0.91.7\nImage codecs: JPEG, PNG\nSigning config: {signing_status}\nTest signing: explicit --test-signing only\nGraphviz: {}",
                env!("CARGO_PKG_VERSION"),
                if std::process::Command::new("dot").arg("-V").output().is_ok() {
                    "available (DOT export can be rendered externally)"
                } else {
                    "not found (DOT export remains available)"
                }
            );
        }
    }
    Ok(())
}
fn fixture() -> DynamicImage {
    DynamicImage::ImageRgba8(image::RgbaImage::from_fn(640, 480, |x, y| {
        image::Rgba([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8, 255])
    }))
}
fn inspect(path: &Path) -> Result<Inspection> {
    let path = canonical_regular_file(path)?;
    let bytes = read_bounded(&path)?;
    let format = image::guess_format(&bytes).context("detecting image format")?;
    if !matches!(format, ImageFormat::Jpeg | ImageFormat::Png) {
        bail!("unsupported image format {format:?}; only JPEG and PNG are supported")
    }
    let image = decode_image(&bytes)?;
    let mut hash = Sha256::new();
    hash.update(&bytes);
    let sha256 = format!("{:x}", hash.finalize());
    let mime = match format {
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Png => "image/png",
        _ => bail!("only JPEG and PNG are supported"),
    };
    let parsed =
        c2pa::Reader::from_context(c2pa::Context::new()).with_stream(mime, Cursor::new(&bytes));
    let (
        credentials,
        integrity,
        trust,
        active_manifest,
        manifest_id,
        claim_generator,
        actions,
        ingredients,
        ingredient_evidence,
        validation,
    ) = match parsed {
        Ok(reader) => {
            let value: serde_json::Value =
                serde_json::from_str(&reader.json()).unwrap_or(serde_json::Value::Null);
            let manifest_obj = reader.active_manifest();
            let active = manifest_obj.and_then(|m| m.label()).map(str::to_owned);
            let manifest = active
                .as_deref()
                .and_then(|a| value.get("manifests").and_then(|m| m.get(a)))
                .unwrap_or(&serde_json::Value::Null);
            let generator = manifest
                .get("claim_generator")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .or_else(|| {
                    manifest
                        .get("claim_generator_info")
                        .and_then(|v| v.as_array())
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(|item| {
                                    let name = item.get("name")?.as_str()?;
                                    Some(match item.get("version").and_then(|v| v.as_str()) {
                                        Some(version) => format!("{name} {version}"),
                                        None => name.to_owned(),
                                    })
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .filter(|s| !s.is_empty())
                });
            let actions = manifest
                .pointer("/assertions")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .filter(|a| {
                    a.get("label")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .contains("actions")
                })
                .flat_map(|a| {
                    a.get("data")
                        .and_then(|d| d.get("actions"))
                        .and_then(|x| x.as_array())
                        .cloned()
                        .unwrap_or_default()
                })
                .filter_map(|a| a.get("action").and_then(|x| x.as_str()).map(str::to_owned))
                .collect();
            let ingredient_evidence: Vec<IngredientEvidence> = manifest_obj
                .map(|m| {
                    m.ingredients()
                        .iter()
                        .map(|i| IngredientEvidence {
                            title: i.title().map(str::to_owned),
                            relationship: i.relationship().as_str().to_owned(),
                            active_manifest: i.active_manifest().map(str::to_owned),
                            validation: i
                                .validation_results()
                                .map(|v| format!("{:?}", v.validation_state())),
                        })
                        .collect()
                })
                .unwrap_or_default();
            let ingredients = ingredient_evidence.len();
            let state = format!("{:?}", reader.validation_state());
            let (integrity, trust) = if state.contains("Trusted") {
                ("valid", "trusted")
            } else if state.contains("Valid") {
                ("valid", "untrusted")
            } else if state.contains("Invalid") {
                ("invalid", "unknown")
            } else {
                ("unknown", "unknown")
            };
            (
                "present",
                integrity,
                trust,
                active.clone(),
                active,
                generator,
                actions,
                ingredients,
                ingredient_evidence,
                value,
            )
        }
        Err(e) => {
            let msg = e.to_string();
            if is_absent_manifest_error(&msg) {
                (
                    "absent",
                    "unknown",
                    "not_evaluated",
                    None,
                    None,
                    None,
                    vec![],
                    0,
                    vec![],
                    serde_json::json!({"note":"No embedded C2PA manifest detected"}),
                )
            } else {
                (
                    "unreadable",
                    "unknown",
                    "unknown",
                    None,
                    None,
                    None,
                    vec![],
                    0,
                    vec![],
                    serde_json::json!({"error":msg}),
                )
            }
        }
    };
    Ok(Inspection {
        path: path.display().to_string(),
        format: format!("{format:?}"),
        width: image.width(),
        height: image.height(),
        bytes: bytes.len() as u64,
        sha256,
        credentials: credentials.to_owned(),
        integrity: integrity.to_owned(),
        trust: trust.to_owned(),
        active_manifest,
        manifest_id,
        claim_generator,
        actions,
        ingredients,
        ingredient_evidence,
        validation,
    })
}
fn print_inspection(r: &Inspection) {
    println!(
        "AI Media Provenance Checker Inspection\n\nAsset: {}\nFormat: {}\nDimensions: {} × {}\nSize: {} bytes\nSHA-256: {}\n\nContent Credentials: {}\nIntegrity: {}\nTrust: {}\nActive manifest: {}\nClaim generator: {}\nIngredients: {}\nActions: {}",
        r.path,
        r.format,
        r.width,
        r.height,
        r.bytes,
        r.sha256,
        r.credentials,
        r.integrity,
        r.trust,
        r.active_manifest.as_deref().unwrap_or("none"),
        r.claim_generator.as_deref().unwrap_or("unknown"),
        r.ingredients,
        if r.actions.is_empty() {
            "none".to_owned()
        } else {
            r.actions.join(", ")
        }
    );
}
fn is_absent_manifest_error(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("no manifest") || normalized.contains("no jumbf data found")
}
fn run(
    input: &Path,
    pipeline: &Path,
    out: &Path,
    mode: Mode,
    test_signing: bool,
    signing_config: Option<&Path>,
    policy_path: Option<&Path>,
) -> Result<()> {
    let cfg: Pipeline =
        serde_yaml_ng::from_slice(&fs::read(pipeline).context("reading pipeline YAML")?)
            .context("invalid pipeline YAML")?;
    validate_pipeline(&cfg)?;
    if matches!(mode, Mode::Reissue | Mode::Compare) && !test_signing && signing_config.is_none() {
        bail!(
            "{} mode requires --signing-config PATH, or explicit --test-signing for an ephemeral untrusted SDK test signer",
            if matches!(mode, Mode::Compare) {
                "compare"
            } else {
                "reissue"
            }
        )
    }
    let signing = if test_signing || signing_config.is_some() {
        Some(signing_context(signing_config, test_signing)?)
    } else {
        None
    };
    let input_real = canonical_regular_file(input)?;
    let source_bytes = read_bounded(&input_real)?;
    let source = inspect(&input_real)?;
    if let Some(expected) = cfg.input.expected_format.as_deref()
        && format_mime(&source.format) != format_mime(expected)
    {
        bail!("input format does not match expected_format {expected}")
    }
    fs::create_dir_all(out)?;
    let report_path = out.join("run.json");
    if report_path.exists() {
        bail!(
            "refusing to overwrite existing run report {}",
            report_path.display()
        )
    }
    let mut artifacts = Vec::new();
    let branches: &[&str] = match mode {
        Mode::Raw => &["raw"],
        Mode::Reissue => &["reissued"],
        Mode::Compare => &["raw", "reissued"],
    };
    for branch in branches {
        let branch_dir = if *branch == "raw" && matches!(mode, Mode::Raw) {
            out.to_path_buf()
        } else {
            out.join(branch)
        };
        fs::create_dir_all(&branch_dir)?;
        let mut parent_bytes = source_bytes.clone();
        let mut parent_inspection = source_ref(&source);
        for (index, step) in cfg.steps.iter().enumerate() {
            let (mut output, output_format, params) = transform(&parent_bytes, step)?;
            let input_hash = sha256_hex(&parent_bytes);
            if *branch == "reissued" && step.kind != "passthrough" {
                let action = action_for(step)?;
                output = sign_derived(
                    signing.as_ref().context("missing signing context")?,
                    &parent_bytes,
                    &format_mime(&parent_inspection.format),
                    &output,
                    &output_format,
                    action,
                    &step.id,
                )?;
            }
            let extension = format_extension(&output_format)?;
            let file_name = format!("{:02}-{}.{}", index + 1, step.id, extension);
            let target = branch_dir.join(file_name);
            atomic_write_new(&target, &output)?;
            let mut inspected = inspect(&target)?;
            inspected.path = fs::canonicalize(&target)?.display().to_string();
            let parent_link = if step.kind == "passthrough" {
                "preserved".into()
            } else {
                parent_link_status(&parent_inspection, &inspected)
            };
            let mut transform = TransformInfo {
                parameters: params,
                input_bytes: parent_bytes.len() as u64,
                output_bytes: output.len() as u64,
            };
            if step.kind == "passthrough" {
                transform.output_bytes = transform.input_bytes;
            }
            artifacts.push(ArtifactRecord {
                branch: (*branch).to_owned(),
                step_id: step.id.clone(),
                operation: step.kind.clone(),
                input_sha256: input_hash,
                transform,
                parent_link,
                inspection: inspected.clone(),
            });
            parent_bytes = output;
            parent_inspection = inspected;
        }
    }
    let mut findings = analyze(&source, &artifacts, matches!(mode, Mode::Compare));
    let policy = policy_path
        .map(load_policy)
        .transpose()?
        .map(|p| evaluate_policy(&artifacts, &p));
    if let Some(result) = &policy {
        for violation in &result.violations {
            findings.push(Finding {
                code: "POLICY_VIOLATION".into(),
                severity: "error".into(),
                message: violation.clone(),
                artifact: None,
                expected: None,
                actual: None,
                remediation: None,
                expected_negative: false,
            });
        }
    }
    let report = RunReport {
        schema_version: "1.0".into(),
        mode: mode_name(mode).into(),
        pipeline_name: cfg.name,
        source,
        artifacts,
        findings,
        policy: policy.clone(),
    };
    atomic_write_new(&report_path, &serde_json::to_vec_pretty(&report)?)?;
    print_run_summary(&report, &report_path);
    if policy.as_ref().is_some_and(|p| !p.passed) {
        bail!(
            "policy failed with {} violation(s)",
            policy.as_ref().map_or(0, |p| p.violations.len())
        )
    }
    Ok(())
}

fn print_run_summary(run: &RunReport, report_path: &Path) {
    println!(
        "AI Media Provenance Checker — Pipeline Analysis\n\nPipeline: {}\nMode: {}\nSource credentials: {} ({})",
        terminal_safe(&run.pipeline_name),
        run.mode,
        run.source.credentials,
        run.source.integrity
    );
    println!("\n{:<20} {:<26} Reissued", "Step", "Raw");
    println!("{}", "─".repeat(78));
    let mut steps = Vec::new();
    for artifact in &run.artifacts {
        if !steps.iter().any(|id| id == &artifact.step_id) {
            steps.push(artifact.step_id.clone());
        }
    }
    for step in steps {
        let raw = run
            .artifacts
            .iter()
            .find(|a| a.branch == "raw" && a.step_id == step);
        let reissued = run
            .artifacts
            .iter()
            .find(|a| a.branch == "reissued" && a.step_id == step);
        println!(
            "{:<20} {:<26} {}",
            terminal_safe(&step),
            raw.map(artifact_status).unwrap_or_else(|| "—".into()),
            reissued.map(artifact_status).unwrap_or_else(|| "—".into())
        );
    }
    let expected_losses = run
        .findings
        .iter()
        .filter(|f| f.code == "CREDENTIALS_MISSING" && f.expected_negative)
        .count();
    let linked = run
        .artifacts
        .iter()
        .filter(|a| a.parent_link == "linked")
        .count();
    println!(
        "\nFindings: {} · Expected raw credential losses: {} · Verified parent links: {}\nRun record: {}",
        run.findings.len(),
        expected_losses,
        linked,
        terminal_safe(&report_path.display().to_string())
    );
    if let Some(policy) = &run.policy {
        println!(
            "Policy: {}",
            if policy.passed { "passed" } else { "failed" }
        );
    }
}

fn artifact_status(artifact: &ArtifactRecord) -> String {
    let mut status = match artifact.inspection.credentials.as_str() {
        "absent" => "Missing".to_owned(),
        "unreadable" => "Unreadable".to_owned(),
        "unsupported" => "Unsupported".to_owned(),
        _ => match artifact.inspection.integrity.as_str() {
            "valid" => "Valid".to_owned(),
            "invalid" => "Invalid".to_owned(),
            _ => "Unknown".to_owned(),
        },
    };
    if artifact.inspection.trust == "untrusted" {
        status.push_str(" (untrusted)");
    }
    if artifact.parent_link == "linked"
        || artifact.parent_link == "preserved"
        || (artifact.branch == "reissued" && artifact.parent_link != "unverifiable")
    {
        status.push_str(&format!(" + {}", artifact.parent_link));
    }
    status
}

fn terminal_safe(value: &str) -> String {
    value
        .chars()
        .flat_map(|c| {
            if c.is_control() {
                format!("\\u{{{:x}}}", c as u32).chars().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

fn source_ref(source: &Inspection) -> Inspection {
    source.clone()
}
fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Raw => "raw",
        Mode::Reissue => "reissue",
        Mode::Compare => "compare",
    }
}
fn validate_pipeline(p: &Pipeline) -> Result<()> {
    if p.version != 1 {
        bail!("unsupported pipeline version {}", p.version)
    }
    if p.steps.is_empty() {
        bail!("pipeline must contain at least one step")
    }
    if p.input.expected_format.as_deref().is_some_and(|format| {
        !["jpeg", "jpg", "image/jpeg", "png", "image/png"]
            .contains(&format.to_ascii_lowercase().as_str())
    }) {
        bail!("pipeline expected_format must be jpeg or png")
    }
    let mut ids = std::collections::HashSet::new();
    for step in &p.steps {
        if step.id.is_empty()
            || !step
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            bail!("step IDs may contain only letters, numbers, hyphens, and underscores")
        }
        if !ids.insert(&step.id) {
            bail!("duplicate pipeline step ID: {}", step.id)
        }
        if !["resize", "crop", "compress", "convert", "passthrough"].contains(&step.kind.as_str()) {
            bail!("unknown pipeline operation: {}", step.kind)
        }
        if step.kind == "resize" && (step.width.is_none() || step.height.is_none()) {
            bail!("resize step {} requires width and height", step.id)
        }
        if step.kind == "crop" && (step.width.is_none() || step.height.is_none()) {
            bail!("crop step {} requires width and height", step.id)
        }
        if step.kind == "convert" && step.format.is_none() {
            bail!("convert step {} requires format", step.id)
        }
        if step.format.as_deref().is_some_and(|f| {
            !["jpeg", "jpg", "image/jpeg", "png", "image/png"]
                .contains(&f.to_ascii_lowercase().as_str())
        }) {
            bail!(
                "step {} has unsupported output format; use jpeg or png",
                step.id
            )
        }
        if step.kind == "resize" || step.kind == "crop" {
            let (width, height) = (step.width.unwrap_or(0), step.height.unwrap_or(0));
            if width == 0
                || height == 0
                || width > 20_000
                || height > 20_000
                || u64::from(width) * u64::from(height) > 100_000_000
            {
                bail!(
                    "step {} dimensions must be positive and at most 100 million pixels",
                    step.id
                )
            }
        }
        if step.kind == "resize"
            && !["exact", "fit"].contains(&step.sizing.as_deref().unwrap_or("exact"))
        {
            bail!("resize sizing must be exact or fit")
        }
        if step.kind == "resize"
            && !["nearest", "triangle", "catmullrom", "lanczos3"]
                .contains(&step.filter.as_deref().unwrap_or("lanczos3"))
        {
            bail!("unsupported resize filter")
        }
        if step.kind == "passthrough" && (step.format.is_some() || step.quality.is_some()) {
            bail!("passthrough does not accept format or quality")
        }
        if step.quality.is_some_and(|q| !(1..=100).contains(&q)) {
            bail!("JPEG quality must be 1 through 100")
        }
    }
    Ok(())
}
fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    if bytes.len() > 100 * 1024 * 1024 {
        bail!("input exceeds 100 MiB limit")
    };
    Ok(bytes)
}
fn canonical_regular_file(path: &Path) -> Result<PathBuf> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("reading {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        bail!("refusing symlink input {}", path.display())
    }
    if !metadata.is_file() {
        bail!("input is not a regular file: {}", path.display())
    }
    Ok(fs::canonicalize(path)?)
}
fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}
fn tamper_jpeg_content_binding(bytes: &mut [u8]) -> Result<()> {
    if bytes.get(0..2) != Some(&[0xff, 0xd8]) {
        bail!("test binding tamper expects JPEG bytes")
    }
    let mut cursor = 2;
    while cursor + 3 < bytes.len() {
        if bytes[cursor] != 0xff {
            bail!("invalid JPEG marker before scan data")
        }
        while cursor < bytes.len() && bytes[cursor] == 0xff {
            cursor += 1;
        }
        let marker = *bytes.get(cursor).context("truncated JPEG marker")?;
        cursor += 1;
        if marker == 0xda {
            let length = u16::from_be_bytes([
                *bytes.get(cursor).context("truncated JPEG scan header")?,
                *bytes
                    .get(cursor + 1)
                    .context("truncated JPEG scan header")?,
            ]) as usize;
            let scan_start = cursor.checked_add(length).context("JPEG offset overflow")?;
            let position = scan_start
                .checked_add(100)
                .context("JPEG offset overflow")?;
            if position >= bytes.len().saturating_sub(2) {
                bail!("JPEG scan is too short for test tampering")
            }
            if bytes[position] == 0xff {
                bail!("test tamper position intersects a JPEG marker")
            }
            bytes[position] ^= 1;
            return Ok(());
        }
        if marker == 0xd8 || marker == 0xd9 || (0xd0..=0xd7).contains(&marker) || marker == 0x01 {
            continue;
        }
        let length = u16::from_be_bytes([
            *bytes.get(cursor).context("truncated JPEG segment")?,
            *bytes.get(cursor + 1).context("truncated JPEG segment")?,
        ]) as usize;
        if length < 2 {
            bail!("invalid JPEG segment length")
        }
        cursor = cursor.checked_add(length).context("JPEG offset overflow")?;
    }
    bail!("JPEG scan data not found")
}
fn format_mime(format: &str) -> String {
    match format.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" | "image/jpeg" => "image/jpeg".into(),
        "png" | "image/png" => "image/png".into(),
        other => other.to_owned(),
    }
}
fn format_extension(format: &str) -> Result<&'static str> {
    match format_mime(format).as_str() {
        "image/jpeg" => Ok("jpg"),
        "image/png" => Ok("png"),
        _ => bail!("unsupported output format {format}; use jpeg or png"),
    }
}
fn atomic_write_new(path: &Path, contents: &[u8]) -> Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().context("output path has no parent")?;
    fs::create_dir_all(parent)?;
    for _ in 0..100 {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let name = path
            .file_name()
            .context("invalid output filename")?
            .to_string_lossy();
        let temp = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), serial));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
        {
            Ok(mut file) => {
                let result = (|| -> Result<()> {
                    file.write_all(contents)?;
                    file.sync_all()?;
                    fs::hard_link(&temp, path)?;
                    Ok(())
                })();
                let _ = fs::remove_file(&temp);
                return result;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    bail!(
        "could not allocate unique temporary output for {}",
        path.display()
    )
}
fn encode_image(
    img: &DynamicImage,
    format: &str,
    quality: Option<u8>,
    background: Option<&str>,
) -> Result<Vec<u8>> {
    match format_mime(format).as_str() {
        "image/jpeg" => {
            let quality = quality.unwrap_or(85);
            if !(1..=100).contains(&quality) {
                bail!("JPEG quality must be between 1 and 100")
            }
            let (r, g, b) = parse_background(background.unwrap_or("#ffffff"))?;
            let rgba = img.to_rgba8();
            let rgb = image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
                let p = rgba.get_pixel(x, y).0;
                let a = u16::from(p[3]);
                image::Rgb([
                    ((u16::from(p[0]) * a + u16::from(r) * (255 - a)) / 255) as u8,
                    ((u16::from(p[1]) * a + u16::from(g) * (255 - a)) / 255) as u8,
                    ((u16::from(p[2]) * a + u16::from(b) * (255 - a)) / 255) as u8,
                ])
            });
            let mut output = Cursor::new(Vec::new());
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, quality)
                .encode_image(&DynamicImage::ImageRgb8(rgb))?;
            Ok(output.into_inner())
        }
        "image/png" => {
            let mut output = Cursor::new(Vec::new());
            img.write_to(&mut output, ImageFormat::Png)?;
            Ok(output.into_inner())
        }
        _ => bail!("unsupported output format {format}"),
    }
}
fn parse_background(color: &str) -> Result<(u8, u8, u8)> {
    let h = color
        .strip_prefix('#')
        .context("JPEG background must be #RRGGBB")?;
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("JPEG background must be #RRGGBB")
    }
    Ok((
        u8::from_str_radix(&h[0..2], 16)?,
        u8::from_str_radix(&h[2..4], 16)?,
        u8::from_str_radix(&h[4..6], 16)?,
    ))
}
fn transform(input: &[u8], step: &Step) -> Result<(Vec<u8>, String, serde_json::Value)> {
    let source_format = image::guess_format(input)?;
    let source_mime = match source_format {
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Png => "image/png",
        _ => bail!("unsupported input format"),
    };
    if step.kind == "passthrough" {
        return Ok((
            input.to_vec(),
            source_mime.into(),
            serde_json::json!({"byte_identical":true}),
        ));
    }
    let mut img = decode_image(input)?;
    let parameters = match step.kind.as_str() {
        "resize" => {
            let before = (img.width(), img.height());
            img = apply_step(img, step)?;
            serde_json::json!({"width":step.width,"height":step.height,"sizing":step.sizing.as_deref().unwrap_or("exact"),"filter":step.filter.as_deref().unwrap_or("lanczos3"),"original_dimensions":before,"result_dimensions":[img.width(),img.height()]})
        }
        "crop" => {
            img = apply_step(img, step)?;
            serde_json::json!({"x":step.x.unwrap_or(0),"y":step.y.unwrap_or(0),"width":step.width,"height":step.height})
        }
        "compress" => {
            serde_json::json!({"quality":step.quality.unwrap_or(85),"encoder":"image-rs JPEG"})
        }
        "convert" => {
            serde_json::json!({"format":step.format,"background":step.background.as_deref().unwrap_or("#ffffff")})
        }
        _ => bail!("unsupported operation"),
    };
    let format = if step.kind == "compress" {
        "jpeg"
    } else {
        step.format
            .as_deref()
            .or_else(|| {
                if source_format == ImageFormat::Jpeg {
                    Some("jpeg")
                } else {
                    Some("png")
                }
            })
            .unwrap()
    };
    let output_mime = format_mime(format);
    let output = encode_image(&img, format, step.quality, step.background.as_deref())?;
    Ok((output, output_mime, parameters))
}
fn action_for(step: &Step) -> Result<&'static str> {
    Ok(match step.kind.as_str() {
        "resize" => "c2pa.resized",
        "crop" => "c2pa.cropped",
        "convert" | "compress" => "c2pa.transcoded",
        other => bail!("no signing action for {other}"),
    })
}
fn parent_link_status(parent: &Inspection, child: &Inspection) -> String {
    let Some(parent_id) = parent.manifest_id.as_deref() else {
        return if child
            .ingredient_evidence
            .iter()
            .any(|i| i.relationship == "parentOf")
        {
            "unverifiable".into()
        } else {
            "missing_parent".into()
        };
    };
    if child
        .ingredient_evidence
        .iter()
        .any(|i| i.relationship == "parentOf" && i.active_manifest.as_deref() == Some(parent_id))
    {
        "linked".into()
    } else if child
        .ingredient_evidence
        .iter()
        .any(|i| i.relationship == "parentOf")
    {
        "wrong_parent".into()
    } else {
        "missing_parent".into()
    }
}
fn analyze(source: &Inspection, artifacts: &[ArtifactRecord], compare: bool) -> Vec<Finding> {
    let mut findings = Vec::new();
    if source.credentials == "absent" {
        findings.push(Finding {
            code: "UNSIGNED_SOURCE".into(),
            severity: "info".into(),
            message:
                "Source has no embedded C2PA credentials; its earlier origin is not established."
                    .into(),
            artifact: Some(source.path.clone()),
            expected: None,
            actual: Some("absent".into()),
            remediation: None,
            expected_negative: false,
        });
    }
    for artifact in artifacts {
        if artifact.inspection.credentials == "unreadable" {
            findings.push(Finding {
                code: "EVIDENCE_INSUFFICIENT".into(),
                severity: "warning".into(),
                message: "The embedded credential data could not be parsed; absence is not established.".into(),
                artifact: Some(artifact.inspection.path.clone()),
                expected: None,
                actual: Some("unreadable".into()),
                remediation: Some("Inspect SDK validation details and verify the asset with a compatible C2PA implementation.".into()),
                expected_negative: false,
            });
        }
        if artifact.inspection.credentials != "present" {
            findings.push(Finding {
                code: "CREDENTIALS_MISSING".into(),
                severity: "error".into(),
                message: "Processed output has no readable embedded credentials.".into(),
                artifact: Some(artifact.inspection.path.clone()),
                expected: Some("present".into()),
                actual: Some(artifact.inspection.credentials.clone()),
                remediation: Some("Use provenance-aware reissue for required provenance.".into()),
                expected_negative: compare && artifact.branch == "raw",
            });
        }
        if artifact.inspection.integrity == "invalid" {
            findings.push(Finding {
                code: "CONTENT_BINDING_INVALID".into(),
                severity: "error".into(),
                message: "Output credentials failed validation.".into(),
                artifact: Some(artifact.inspection.path.clone()),
                expected: Some("valid".into()),
                actual: Some("invalid".into()),
                remediation: None,
                expected_negative: false,
            });
        }
        if artifact.inspection.trust == "untrusted" {
            findings.push(Finding {
                code: "SIGNATURE_UNTRUSTED".into(),
                severity: "warning".into(),
                message: "Cryptographic validation succeeded but signer trust is not established."
                    .into(),
                artifact: Some(artifact.inspection.path.clone()),
                expected: None,
                actual: Some("untrusted".into()),
                remediation: None,
                expected_negative: false,
            });
        }
        if artifact.parent_link == "linked" {
            findings.push(Finding {
                code: "CHAIN_COMPLETE".into(),
                severity: "info".into(),
                message: "Output manifest references the expected immediate parent manifest."
                    .into(),
                artifact: Some(artifact.inspection.path.clone()),
                expected: Some("linked".into()),
                actual: Some("linked".into()),
                remediation: None,
                expected_negative: false,
            });
        } else if artifact.branch == "reissued" || compare {
            findings.push(Finding {
                code: if artifact.parent_link == "wrong_parent" {
                    "PARENT_LINK_MISMATCH"
                } else {
                    "PARENT_LINK_MISSING"
                }
                .into(),
                severity: "error".into(),
                message: "Expected immediate-parent provenance could not be verified.".into(),
                artifact: Some(artifact.inspection.path.clone()),
                expected: Some("linked".into()),
                actual: Some(artifact.parent_link.clone()),
                remediation: None,
                expected_negative: false,
            });
        }
        let expected_action = match artifact.operation.as_str() {
            "resize" => Some("c2pa.resized"),
            "crop" => Some("c2pa.cropped"),
            "convert" | "compress" => Some("c2pa.transcoded"),
            _ => None,
        };
        if artifact.branch == "reissued"
            && expected_action
                .is_some_and(|action| !artifact.inspection.actions.iter().any(|a| a == action))
        {
            findings.push(Finding {
                code: "TRANSFORMATION_ACTION_MISSING".into(),
                severity: "error".into(),
                message: "Reissued manifest is missing the action expected for this operation."
                    .into(),
                artifact: Some(artifact.inspection.path.clone()),
                expected: expected_action.map(str::to_owned),
                actual: Some(artifact.inspection.actions.join(", ")),
                remediation: Some(
                    "Record the corresponding C2PA action when creating the derived manifest."
                        .into(),
                ),
                expected_negative: false,
            });
        }
    }
    findings
}

fn load_policy(path: &Path) -> Result<PolicyFile> {
    let policy: PolicyFile = serde_yaml_ng::from_slice(&fs::read(path).context("reading policy")?)
        .context("invalid policy YAML")?;
    if policy.version.is_some_and(|v| v != 1) {
        bail!("unsupported policy version")
    }
    Ok(policy)
}
fn evaluate_policy(artifacts: &[ArtifactRecord], policy: &PolicyFile) -> PolicyResult {
    let branch = if artifacts.iter().any(|a| a.branch == "reissued") {
        "reissued"
    } else {
        "raw"
    };
    let mut violations = Vec::new();
    for a in artifacts.iter().filter(|a| a.branch == branch) {
        let name = format!("{} ({})", a.step_id, a.inspection.path);
        let r = &policy.requirements;
        if r.require_output_credentials && a.inspection.credentials != "present" {
            violations.push(format!(
                "{name}: output credentials are {}",
                a.inspection.credentials
            ));
        }
        if r.require_valid_integrity && a.inspection.integrity != "valid" {
            violations.push(format!("{name}: integrity is {}", a.inspection.integrity));
        }
        if r.require_trusted_signer && a.inspection.trust != "trusted" {
            violations.push(format!("{name}: signer trust is {}", a.inspection.trust));
        }
        if r.require_parent_link
            && a.operation != "passthrough"
            && a.parent_link != "linked"
            && (a.parent_link != "unverifiable" || policy.behavior.fail_on_unverifiable)
        {
            violations.push(format!("{name}: parent link is {}", a.parent_link));
        }
        let expected = match a.operation.as_str() {
            "resize" => Some("c2pa.resized"),
            "crop" => Some("c2pa.cropped"),
            "convert" | "compress" => Some("c2pa.transcoded"),
            _ => None,
        };
        if r.require_transformation_action
            && expected.is_some_and(|e| !a.inspection.actions.iter().any(|v| v == e))
        {
            violations.push(format!("{name}: expected action assertion is missing"));
        }
    }
    PolicyResult {
        passed: violations.is_empty(),
        violations,
    }
}
fn verify(path: &Path, policy_path: Option<&Path>) -> Result<()> {
    let mut stored: RunReport =
        serde_json::from_slice(&fs::read(path).context("reading run report")?)
            .context("invalid run report JSON")?;
    if stored.schema_version != "1.0" {
        bail!("unsupported run report schema version")
    }
    let source = inspect(Path::new(&stored.source.path)).context("re-inspecting source")?;
    if source.sha256 != stored.source.sha256 {
        bail!("source hash changed since run")
    }
    let mut parents = std::collections::HashMap::new();
    let mut fresh = Vec::new();
    for mut a in stored.artifacts {
        let current = inspect(Path::new(&a.inspection.path))
            .with_context(|| format!("re-inspecting {}", a.inspection.path))?;
        if current.sha256 != a.inspection.sha256 {
            bail!("artifact hash changed: {}", a.inspection.path)
        }
        let parent = parents.get(&a.branch).unwrap_or(&source);
        if parent.sha256 != a.input_sha256 {
            bail!("stale parent hash in report for {}", a.step_id)
        }
        a.parent_link = if a.operation == "passthrough" {
            "preserved".into()
        } else {
            parent_link_status(parent, &current)
        };
        a.inspection = current.clone();
        parents.insert(a.branch.clone(), current);
        println!("OK  {}", a.inspection.path);
        fresh.push(a);
    }
    stored.source = source;
    stored.artifacts = fresh;
    stored.findings = analyze(&stored.source, &stored.artifacts, stored.mode == "compare");
    if let Some(path) = policy_path {
        stored.policy = Some(evaluate_policy(&stored.artifacts, &load_policy(path)?));
    }
    if stored.policy.as_ref().is_some_and(|p| !p.passed) {
        bail!(
            "policy failed: {}",
            stored
                .policy
                .as_ref()
                .map(|p| p.violations.join("; "))
                .unwrap_or_default()
        )
    }
    println!(
        "Verified source and {} output artifacts.",
        stored.artifacts.len()
    );
    Ok(())
}
fn compare(source_path: &Path, derived_path: &Path, policy_path: Option<&Path>) -> Result<()> {
    let source = inspect(source_path)?;
    let derived = inspect(derived_path)?;
    let link = parent_link_status(&source, &derived);
    let artifact = ArtifactRecord {
        branch: "reissued".into(),
        step_id: "derived".into(),
        operation: "compare".into(),
        input_sha256: source.sha256.clone(),
        transform: TransformInfo {
            parameters: serde_json::Value::Null,
            input_bytes: source.bytes,
            output_bytes: derived.bytes,
        },
        parent_link: link.clone(),
        inspection: derived.clone(),
    };
    let policy = policy_path
        .map(load_policy)
        .transpose()?
        .map(|p| evaluate_policy(std::slice::from_ref(&artifact), &p));
    let findings = analyze(&source, std::slice::from_ref(&artifact), false);
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"source":source,"derived":derived,"parent_link":link,"findings":findings,"policy":policy})
        )?
    );
    if policy.as_ref().is_some_and(|p| !p.passed) {
        bail!(
            "comparison policy failed: {}",
            policy
                .as_ref()
                .map(|result| result.violations.join("; "))
                .unwrap_or_default()
        )
    };
    Ok(())
}

fn report(run_path: &Path, format: ReportFormat, output: &Path) -> Result<()> {
    let stored: RunReport =
        serde_json::from_slice(&fs::read(run_path).context("reading run report")?)
            .context("invalid run report JSON")?;
    let contents = match format {
        ReportFormat::Json => serde_json::to_string_pretty(&stored)?,
        ReportFormat::Html => render_html(&stored),
        ReportFormat::Dot => render_dot(&stored),
    };
    atomic_write_new(output, contents.as_bytes()).with_context(|| {
        format!(
            "writing {} (existing outputs are never overwritten)",
            output.display()
        )
    })?;
    println!(
        "Wrote {} report: {}",
        match format {
            ReportFormat::Json => "JSON",
            ReportFormat::Html => "HTML",
            ReportFormat::Dot => "DOT",
        },
        output.display()
    );
    Ok(())
}
fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn render_html(run: &RunReport) -> String {
    let raw_missing = run
        .findings
        .iter()
        .filter(|f| f.code == "CREDENTIALS_MISSING" && f.expected_negative)
        .count();
    let invalid = run
        .artifacts
        .iter()
        .filter(|a| a.inspection.integrity == "invalid")
        .count();
    let linked = run
        .artifacts
        .iter()
        .filter(|a| a.parent_link == "linked")
        .count();
    let validations = run
        .artifacts
        .iter()
        .filter(|a| a.inspection.credentials == "present")
        .count();
    let policy_violations = run
        .findings
        .iter()
        .filter(|f| f.code == "POLICY_VIOLATION")
        .count();
    let policy = run
        .policy
        .as_ref()
        .map(|p| if p.passed { "Passed" } else { "Failed" })
        .unwrap_or("Not evaluated");
    let mut body = String::from(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>AI Media Provenance Checker report</title><style>body{font:15px system-ui,sans-serif;max-width:1100px;margin:2rem auto;padding:0 1rem;color:#17212b}table{border-collapse:collapse;width:100%;margin:1rem 0}th,td{border:1px solid #ccd3da;padding:.55rem;text-align:left;vertical-align:top}th{background:#f0f3f6}code{overflow-wrap:anywhere}.status{font-weight:650}.branch{margin-top:2rem}.graph{overflow-x:auto;border:1px solid #ccd3da;padding:.75rem}.graph svg{max-width:none}</style><body><h1>AI Media Provenance Checker run</h1>",
    );
    body.push_str(&format!("<p>Pipeline: <strong>{}</strong> · Mode: <strong>{}</strong> · Policy: <strong>{}</strong></p><p>Source: <code>{}</code> · {} · {} × {} · Manifest <code>{}</code> · SHA-256 <code>{}</code></p>",html_escape(&run.pipeline_name),html_escape(&run.mode),policy,html_escape(&run.source.path),html_escape(&run.source.credentials),run.source.width,run.source.height,html_escape(run.source.manifest_id.as_deref().unwrap_or("none")),html_escape(&run.source.sha256)));
    body.push_str(&format!("<h2>Overview</h2><p>Artifacts: {} · Manifest validations: {} · Raw credential losses: {} · Invalid credentials: {} · Verified parent links: {} · Policy violations: {}</p>", run.artifacts.len(), validations, raw_missing, invalid, linked, policy_violations));
    body.push_str("<h2>Provenance graph</h2><div class=\"graph\">");
    body.push_str(&render_graph_svg(run));
    body.push_str("</div><p>Solid edges show a verified ingredient link or byte-identical passthrough; dashed edges show observed processing without verified signed continuity.</p>");
    body.push_str("<h2>Processing stages</h2>");
    for branch in ["raw", "reissued"] {
        let records: Vec<_> = run
            .artifacts
            .iter()
            .filter(|a| a.branch == branch)
            .collect();
        if records.is_empty() {
            continue;
        }
        body.push_str(&format!("<section class=\"branch\"><h3>{}</h3><table><thead><tr><th>Step</th><th>Asset</th><th>Credentials</th><th>Integrity</th><th>Trust</th><th>Parent</th><th>Manifest ID</th><th>Actions</th><th>Validation codes</th><th>SHA-256</th></tr></thead><tbody>",html_escape(branch)));
        for a in records {
            let i = &a.inspection;
            let validation_codes = i
                .validation
                .get("validation_status")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .filter_map(|item| item.get("code").and_then(|v| v.as_str()))
                .collect::<Vec<_>>()
                .join(", ");
            body.push_str(&format!("<tr><td>{} ({})</td><td>{} · {} × {}</td><td class=\"status\">{}</td><td>{}</td><td>{}</td><td>{}</td><td><code>{}</code></td><td>{}</td><td>{}</td><td><code>{}</code></td></tr>",html_escape(&a.step_id),html_escape(&a.operation),html_escape(&i.format),i.width,i.height,html_escape(&i.credentials),html_escape(&i.integrity),html_escape(&i.trust),html_escape(&a.parent_link),html_escape(i.manifest_id.as_deref().unwrap_or("unknown")),html_escape(&i.actions.join(", ")),html_escape(&validation_codes),html_escape(&i.sha256)));
        }
        body.push_str("</tbody></table></section>");
    }
    body.push_str("<h2>Findings</h2><ul>");
    for f in &run.findings {
        body.push_str(&format!(
            "<li><strong>{}</strong> [{}] {}{}</li>",
            html_escape(&f.code),
            html_escape(&f.severity),
            html_escape(&f.message),
            if f.expected_negative {
                " (expected negative branch)"
            } else {
                ""
            }
        ));
    }
    body.push_str("</ul><h2>Recommendations</h2><ul>");
    if raw_missing > 0 {
        body.push_str("<li>For pipelines that require provenance, use a supported reissue path and verify the new manifest and immediate-parent ingredient.</li>");
    }
    if invalid > 0 {
        body.push_str("<li>Quarantine outputs with invalid content bindings and investigate where signed media bytes changed.</li>");
    }
    if run
        .artifacts
        .iter()
        .any(|a| a.inspection.trust == "untrusted")
    {
        body.push_str("<li>Review signer trust under the trust policy appropriate for your deployment; cryptographic validity alone does not establish trusted identity.</li>");
    }
    if raw_missing == 0 && invalid == 0 && linked == run.artifacts.len() {
        body.push_str("<li>No missing credentials, invalid bindings, or unlinked parents were detected in the recorded stages.</li>");
    }
    body.push_str("</ul><h2>Interpretation limits</h2><p>Valid signatures establish integrity and attribution to a signing key. They do not prove assertions are factually true. An unsigned source does not establish origin. Trust is reported separately from cryptographic integrity.</p></body></html>");
    body
}
fn render_graph_svg(run: &RunReport) -> String {
    let branches: Vec<_> = ["raw", "reissued"]
        .into_iter()
        .filter_map(|name| {
            let records: Vec<_> = run.artifacts.iter().filter(|a| a.branch == name).collect();
            (!records.is_empty()).then_some((name, records))
        })
        .collect();
    let max_steps = branches
        .iter()
        .map(|(_, rows)| rows.len())
        .max()
        .unwrap_or(0);
    let width = 210 + max_steps as u32 * 215;
    let height = (branches.len() as u32 * 125).max(110);
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" role=\"img\" aria-label=\"Recorded image processing and provenance relationships\" viewBox=\"0 0 {width} {height}\" width=\"{width}\" height=\"{height}\"><style>text{{font:12px system-ui,sans-serif;fill:#17212b}}.title{{font-weight:700}}.ok{{stroke:#18804b}}.weak{{stroke:#9a5b00;stroke-dasharray:6 4}}.node{{fill:#fff;stroke-width:2}}.edge{{stroke-width:2}}</style>"
    );
    for (lane, (branch, records)) in branches.iter().enumerate() {
        let y = 25 + lane as u32 * 125;
        svg.push_str(&format!(
            "<text x=\"16\" y=\"{}\" class=\"title\">{}</text>",
            y + 10,
            html_escape(branch)
        ));
        let sy = y + 20;
        svg.push_str(&format!("<rect class=\"node\" x=\"10\" y=\"{}\" width=\"150\" height=\"78\" rx=\"6\" stroke=\"#536579\"/><text x=\"20\" y=\"{}\" class=\"title\">Source</text><text x=\"20\" y=\"{}\">{} · {}×{}</text><text x=\"20\" y=\"{}\">Credentials: {}</text>", sy, sy + 23, sy + 43, html_escape(&run.source.format), run.source.width, run.source.height, sy + 62, html_escape(&run.source.credentials)));
        let mut previous_x = 160;
        for (index, asset) in records.iter().enumerate() {
            let x = 190 + index as u32 * 215;
            let status_class = if asset.parent_link == "linked" || asset.parent_link == "preserved"
            {
                "ok"
            } else {
                "weak"
            };
            svg.push_str(&format!("<line class=\"edge {status_class}\" x1=\"{previous_x}\" y1=\"{}\" x2=\"{x}\" y2=\"{}\"/><rect class=\"node\" x=\"{x}\" y=\"{sy}\" width=\"195\" height=\"78\" rx=\"6\" stroke=\"{}\"/><text x=\"{}\" y=\"{}\" class=\"title\">{}</text><text x=\"{}\" y=\"{}\">{} · {}×{}</text><text x=\"{}\" y=\"{}\">{} / {} · {}</text>", sy + 59, sy + 59, if status_class == "ok" { "#18804b" } else { "#9a5b00" }, x + 10, sy + 20, html_escape(&asset.step_id), x + 10, sy + 40, html_escape(&asset.inspection.format), asset.inspection.width, asset.inspection.height, x + 10, sy + 60, html_escape(&asset.inspection.credentials), html_escape(&asset.inspection.integrity), html_escape(&asset.parent_link)));
            previous_x = x + 195;
        }
    }
    svg.push_str("</svg>");
    svg
}
fn render_dot(run: &RunReport) -> String {
    let mut dot = String::from("digraph ai_media_provenance_checker {\n  rankdir=LR;\n");
    dot.push_str(&format!(
        "  source [shape=box,label=\"{}\\n{} / {}\"];\n",
        dot_escape(&run.source.path),
        run.source.integrity,
        run.source.credentials
    ));
    let mut last = std::collections::HashMap::<String, String>::new();
    for (index, asset) in run.artifacts.iter().enumerate() {
        let i = &asset.inspection;
        dot.push_str(&format!(
            "  artifact_{index} [label=\"{}\\n{} × {}\\n{} / {}\"];\n",
            dot_escape(&asset.step_id),
            i.width,
            i.height,
            i.integrity,
            i.credentials
        ));
        let from = last
            .get(&asset.branch)
            .cloned()
            .unwrap_or_else(|| "source".into());
        let style = if asset.parent_link == "linked" || asset.parent_link == "preserved" {
            "solid"
        } else {
            "dashed"
        };
        dot.push_str(&format!(
            "  {from} -> artifact_{index} [label=\"{}\",style={style}];\n",
            dot_escape(&asset.parent_link)
        ));
        last.insert(asset.branch.clone(), format!("artifact_{index}"));
    }
    dot.push_str("}\n");
    dot
}
fn dot_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}
fn signing_context(
    config: Option<&Path>,
    test_signing: bool,
) -> Result<std::sync::Arc<C2paContext>> {
    if test_signing {
        let signer = c2pa::EphemeralSigner::new("ai-media-provenance-checker.test")?;
        return Ok(C2paContext::new().with_signer(signer).into_shared());
    }
    let config =
        config.context("reissue mode requires --signing-config PATH or explicit --test-signing")?;
    let settings = Settings::new().with_file(config)?;
    Ok(C2paContext::new().with_settings(settings)?.into_shared())
}
fn sign_created(
    context: &std::sync::Arc<C2paContext>,
    bytes: &[u8],
    format: &str,
) -> Result<Vec<u8>> {
    let mut builder = Builder::from_shared_context(context).with_definition(serde_json::json!({
        "title":"AI Media Provenance Checker deterministic test fixture", "format":format,
        "claim_generator_info":[{"name":"AIMediaProvenanceChecker","version":env!("CARGO_PKG_VERSION")}]
    }))?;
    builder.add_action(serde_json::json!({"action":"c2pa.created","digitalSourceType":"http://cv.iptc.org/newscodes/digitalsourcetype/digitalArt","softwareAgent":{"name":"AIMediaProvenanceChecker","version":env!("CARGO_PKG_VERSION")}}))?;
    let mut source = Cursor::new(bytes);
    let mut dest = Cursor::new(Vec::new());
    builder.save_to_stream(format, &mut source, &mut dest)?;
    Ok(dest.into_inner())
}
fn sign_derived(
    context: &std::sync::Arc<C2paContext>,
    parent: &[u8],
    parent_format: &str,
    output: &[u8],
    output_format: &str,
    action: &str,
    title: &str,
) -> Result<Vec<u8>> {
    let mut builder = Builder::from_shared_context(context).with_definition(serde_json::json!({
        "title":title, "format":output_format,
        "claim_generator_info":[{"name":"AIMediaProvenanceChecker","version":env!("CARGO_PKG_VERSION")}]
    }))?;
    builder.set_intent(BuilderIntent::Edit);
    builder.add_ingredient_from_stream(
        serde_json::json!({"title":title,"relationship":"parentOf","label":"parent"}).to_string(),
        parent_format,
        &mut Cursor::new(parent),
    )?;
    builder.add_action(serde_json::json!({"action":action,"softwareAgent":{"name":"AIMediaProvenanceChecker","version":env!("CARGO_PKG_VERSION")}}))?;
    let mut source = Cursor::new(output);
    let mut dest = Cursor::new(Vec::new());
    builder.save_to_stream(output_format, &mut source, &mut dest)?;
    Ok(dest.into_inner())
}
fn apply_step(img: DynamicImage, s: &Step) -> Result<DynamicImage> {
    Ok(match s.kind.as_str() {
        "resize" => {
            let (w, h) = (
                s.width.context("resize requires width")?,
                s.height.context("resize requires height")?,
            );
            if w == 0
                || h == 0
                || w > 20_000
                || h > 20_000
                || u64::from(w) * u64::from(h) > 100_000_000
            {
                bail!("resize dimensions exceed safe limits")
            };
            let f = match s.filter.as_deref().unwrap_or("lanczos3") {
                "nearest" => FilterType::Nearest,
                "triangle" => FilterType::Triangle,
                "catmullrom" => FilterType::CatmullRom,
                "lanczos3" => FilterType::Lanczos3,
                v => bail!("unknown resize filter: {v}"),
            };
            match s.sizing.as_deref().unwrap_or("exact") {
                "exact" => img.resize_exact(w, h, f),
                "fit" => img.resize(w, h, f),
                other => bail!("unknown sizing policy: {other}"),
            }
        }
        "crop" => {
            let x = s.x.unwrap_or(0);
            let y = s.y.unwrap_or(0);
            let w = s.width.context("crop requires width")?;
            let h = s.height.context("crop requires height")?;
            if w == 0
                || h == 0
                || x.checked_add(w).is_none_or(|v| v > img.width())
                || y.checked_add(h).is_none_or(|v| v > img.height())
            {
                bail!("crop rectangle is outside the input image")
            };
            img.crop_imm(x, y, w, h)
        }
        "compress" | "convert" => img,
        other => bail!("unknown pipeline operation: {other}"),
    })
}
fn decode_image(bytes: &[u8]) -> Result<DynamicImage> {
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .context("unsupported or malformed image")?;
    let (width, height) = reader
        .into_dimensions()
        .context("reading image dimensions")?;
    if width == 0
        || height == 0
        || width > 20_000
        || height > 20_000
        || u64::from(width) * u64::from(height) > 100_000_000
    {
        bail!("image dimensions exceed safe limits")
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .context("unsupported or malformed image")?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(20_000);
    limits.max_image_height = Some(20_000);
    reader.limits(limits);
    reader.decode().context("decoding image")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognizes_sdk_missing_jumbf_result_as_absent() {
        assert!(is_absent_manifest_error("no JUMBF data found"));
        assert!(!is_absent_manifest_error("invalid JUMBF assertion"));
    }
    #[test]
    fn resize_forces_requested_dimensions() {
        let result = apply_step(
            fixture(),
            &Step {
                id: "r".into(),
                kind: "resize".into(),
                width: Some(24),
                height: Some(12),
                x: None,
                y: None,
                filter: None,
                quality: None,
                format: None,
                sizing: None,
                background: None,
            },
        )
        .unwrap();
        assert_eq!((result.width(), result.height()), (24, 12));
    }
    #[test]
    fn resize_fit_preserves_aspect_ratio_within_bounds() {
        let result = apply_step(
            fixture(),
            &Step {
                id: "fit".into(),
                kind: "resize".into(),
                width: Some(100),
                height: Some(100),
                x: None,
                y: None,
                filter: Some("lanczos3".into()),
                quality: None,
                format: None,
                sizing: Some("fit".into()),
                background: None,
            },
        )
        .unwrap();
        assert_eq!((result.width(), result.height()), (100, 75));
    }
    #[test]
    fn crop_rejects_out_of_bounds_rectangle() {
        let result = apply_step(
            fixture(),
            &Step {
                id: "c".into(),
                kind: "crop".into(),
                width: Some(24),
                height: Some(12),
                x: Some(639),
                y: Some(0),
                filter: None,
                quality: None,
                format: None,
                sizing: None,
                background: None,
            },
        );
        assert!(result.is_err());
    }
    #[test]
    fn html_report_escapes_asset_paths() {
        let malicious = Inspection {
            path: "<img src=x onerror=alert(1)>".into(),
            format: "PNG".into(),
            width: 1,
            height: 1,
            bytes: 1,
            sha256: "abc".into(),
            credentials: "absent".into(),
            integrity: "unknown".into(),
            trust: "unknown".into(),
            active_manifest: None,
            manifest_id: None,
            claim_generator: None,
            actions: vec![],
            ingredients: 0,
            ingredient_evidence: vec![],
            validation: serde_json::Value::Null,
        };
        let report = RunReport {
            schema_version: "1.0".into(),
            mode: "raw".into(),
            pipeline_name: "test".into(),
            source: malicious.clone(),
            artifacts: vec![ArtifactRecord {
                branch: "raw".into(),
                step_id: "test".into(),
                operation: "crop".into(),
                input_sha256: "src".into(),
                transform: TransformInfo {
                    parameters: serde_json::Value::Null,
                    input_bytes: 1,
                    output_bytes: 1,
                },
                parent_link: "unverifiable".into(),
                inspection: malicious,
            }],
            findings: vec![],
            policy: None,
        };
        assert!(!render_html(&report).contains("<img src=x"));
        assert!(render_html(&report).contains("&lt;img"));
    }

    #[test]
    fn pipeline_rejects_duplicate_ids_and_unknown_fields() {
        let yaml = r#"
version: 1
steps:
  - id: same
    type: resize
    width: 10
    height: 10
  - id: same
    type: crop
    width: 2
    height: 2
"#;
        let parsed: Pipeline = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(validate_pipeline(&parsed).is_err());

        let typo = r#"version: 1
steps:
  - id: x
    type: resize
    width: 10
    height: 10
    widht: 4
"#;
        assert!(serde_yaml_ng::from_str::<Pipeline>(typo).is_err());
    }

    #[test]
    fn pipeline_rejects_oversized_dimensions_and_unknown_formats() {
        let oversized = Pipeline {
            version: 1,
            name: "test".into(),
            input: PipelineInput::default(),
            steps: vec![Step {
                id: "large".into(),
                kind: "resize".into(),
                width: Some(20_001),
                height: Some(1),
                x: None,
                y: None,
                filter: None,
                quality: None,
                format: None,
                sizing: None,
                background: None,
            }],
        };
        assert!(validate_pipeline(&oversized).is_err());
        let bad_format = Pipeline {
            version: 1,
            name: "test".into(),
            input: PipelineInput::default(),
            steps: vec![Step {
                id: "convert".into(),
                kind: "convert".into(),
                width: None,
                height: None,
                x: None,
                y: None,
                filter: None,
                quality: None,
                format: Some("webp".into()),
                sizing: None,
                background: None,
            }],
        };
        assert!(validate_pipeline(&bad_format).is_err());
    }
}
