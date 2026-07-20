use std::{
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

const WARMUP_RUNS: usize = 3;
const MEASURED_RUNS: usize = 5;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const UNSET: u64 = u64::MAX;

pub(super) fn measure(binary: &Path, fixture: &Path, workspace: &Path) -> Result<()> {
    let scenarios = [
        StartupScenario::mock(workspace, fixture),
        StartupScenario::git(1)?,
        StartupScenario::git(500)?,
    ];
    println!("Diffo release startup measurement: 100x30 PTY, 3 warmups, 5 samples");
    println!("scenario          run first_output_ms ready_ms");
    for scenario in &scenarios {
        for _ in 0..WARMUP_RUNS {
            measure_once(binary, scenario)?;
        }
        let mut measurements = Vec::with_capacity(MEASURED_RUNS);
        for run in 1..=MEASURED_RUNS {
            let measurement = measure_once(binary, scenario)?;
            print_measurement(scenario.name, &run.to_string(), measurement);
            measurements.push(measurement);
        }
        print_measurement(scenario.name, "median", median(&measurements));
    }
    Ok(())
}

struct StartupScenario {
    name: &'static str,
    cwd: PathBuf,
    mock_file: Option<PathBuf>,
    ready_marker: &'static [u8],
    _temporary: Option<tempfile::TempDir>,
}

impl StartupScenario {
    fn mock(workspace: &Path, fixture: &Path) -> Self {
        Self {
            name: "mock-5.6m-lines",
            cwd: workspace.to_path_buf(),
            mock_file: Some(fixture.to_path_buf()),
            ready_marker: b"src/main.rs",
            _temporary: None,
        }
    }

    fn git(file_count: usize) -> Result<Self> {
        let temporary = tempfile::tempdir().context("create startup repository")?;
        prepare_git_repository(temporary.path(), file_count)?;
        let name = match file_count {
            1 => "git-1-change",
            500 => "git-500-changes",
            _ => unreachable!("startup workloads use fixed file counts"),
        };
        Ok(Self {
            name,
            cwd: temporary.path().to_path_buf(),
            mock_file: None,
            ready_marker: b"startup-0000.txt",
            _temporary: Some(temporary),
        })
    }
}

#[derive(Clone, Copy)]
struct StartupMeasurement {
    first_output_us: u64,
    ready_us: u64,
}

struct ObservedMilestones {
    first_output_us: AtomicU64,
    ready_us: AtomicU64,
}

impl ObservedMilestones {
    fn new() -> Self {
        Self {
            first_output_us: AtomicU64::new(UNSET),
            ready_us: AtomicU64::new(UNSET),
        }
    }

    fn measurement(&self) -> Option<StartupMeasurement> {
        let first_output_us = self.first_output_us.load(Ordering::Acquire);
        let ready_us = self.ready_us.load(Ordering::Acquire);
        (first_output_us != UNSET && ready_us != UNSET).then_some(StartupMeasurement {
            first_output_us,
            ready_us,
        })
    }
}

fn measure_once(binary: &Path, scenario: &StartupScenario) -> Result<StartupMeasurement> {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 30,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("open startup PTY")?;
    let mut reader = pair
        .master
        .try_clone_reader()
        .context("clone startup PTY reader")?;
    let mut writer = pair
        .master
        .take_writer()
        .context("open startup PTY writer")?;
    let milestones = Arc::new(ObservedMilestones::new());
    let reader_milestones = Arc::clone(&milestones);
    let ready_marker = scenario.ready_marker;
    let mut command = CommandBuilder::new(binary.as_os_str());
    command.cwd(scenario.cwd.as_os_str());
    command.env("TERM", "xterm-256color");
    if let Some(mock_file) = &scenario.mock_file {
        command.env("DIFFO_MOCK_FILE", mock_file.as_os_str());
    }
    let started = Instant::now();
    let mut child = pair
        .slave
        .spawn_command(command)
        .context("launch startup Diffo")?;
    drop(pair.slave);
    let reader_thread = thread::spawn(move || {
        let mut received = Vec::new();
        let mut buffer = [0_u8; 8192];
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 {
                break;
            }
            let elapsed = elapsed_us(started);
            let _ = reader_milestones.first_output_us.compare_exchange(
                UNSET,
                elapsed,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
            received.extend_from_slice(&buffer[..count]);
            record_marker(
                &reader_milestones.ready_us,
                &received,
                ready_marker,
                elapsed,
            );
        }
    });

    let deadline = Instant::now() + STARTUP_TIMEOUT;
    let measurement = loop {
        if let Some(measurement) = milestones.measurement() {
            break measurement;
        }
        ensure!(
            child.try_wait().context("poll startup Diffo")?.is_none(),
            "Diffo exited before startup milestones for {}",
            scenario.name
        );
        ensure!(
            Instant::now() < deadline,
            "Diffo startup timed out for {}",
            scenario.name
        );
        thread::sleep(Duration::from_millis(1));
    };

    writer.write_all(b"q").context("quit startup Diffo")?;
    writer.flush().context("flush startup quit")?;
    let status = child.wait().context("wait for startup Diffo")?;
    ensure!(status.success(), "startup Diffo failed: {status:?}");
    drop(writer);
    reader_thread
        .join()
        .map_err(|_| anyhow::anyhow!("startup PTY reader panicked"))?;
    Ok(measurement)
}

fn record_marker(target: &AtomicU64, output: &[u8], marker: &[u8], elapsed: u64) {
    if target.load(Ordering::Acquire) == UNSET
        && output.windows(marker.len()).any(|window| window == marker)
    {
        let _ = target.compare_exchange(UNSET, elapsed, Ordering::AcqRel, Ordering::Acquire);
    }
}

fn elapsed_us(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

fn prepare_git_repository(path: &Path, file_count: usize) -> Result<()> {
    run_git(path, &["init", "--quiet"])?;
    run_git(path, &["config", "user.email", "startup@example.invalid"])?;
    run_git(path, &["config", "user.name", "Diffo Startup"])?;
    for index in 0..file_count {
        fs::write(path.join(format!("startup-{index:04}.txt")), "original\n")
            .context("write startup baseline")?;
    }
    run_git(path, &["add", "."])?;
    run_git(path, &["commit", "--quiet", "-m", "startup baseline"])?;
    for index in 0..file_count {
        fs::write(path.join(format!("startup-{index:04}.txt")), "changed\n")
            .context("write startup changes")?;
    }
    Ok(())
}

fn run_git(path: &Path, arguments: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(path)
        .output()
        .with_context(|| format!("run git {}", arguments.join(" ")))?;
    ensure!(
        output.status.success(),
        "git {} failed: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

#[allow(clippy::cast_precision_loss)]
fn print_measurement(scenario: &str, run: &str, measurement: StartupMeasurement) {
    println!(
        "{scenario:<17} {run:>6} {:>15.1} {:>8.1}",
        measurement.first_output_us as f64 / 1000.0,
        measurement.ready_us as f64 / 1000.0,
    );
}

fn median(measurements: &[StartupMeasurement]) -> StartupMeasurement {
    fn middle(mut values: Vec<u64>) -> u64 {
        values.sort_unstable();
        values[values.len() / 2]
    }
    StartupMeasurement {
        first_output_us: middle(
            measurements
                .iter()
                .map(|measurement| measurement.first_output_us)
                .collect(),
        ),
        ready_us: middle(
            measurements
                .iter()
                .map(|measurement| measurement.ready_us)
                .collect(),
        ),
    }
}
