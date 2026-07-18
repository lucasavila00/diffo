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
use serde::Deserialize;

const SETTLE_TIME: Duration = Duration::from_secs(1);
const SAMPLE_TIME: Duration = Duration::from_secs(5);
const WARMUP_RUNS: usize = 3;
const MEASURED_RUNS: usize = 5;

#[derive(Clone, Copy)]
enum Scenario {
    Idle,
    Scroll,
}

impl Scenario {
    const ALL: [Self; 2] = [Self::Idle, Self::Scroll];

    fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Scroll => "scroll",
        }
    }
}

#[derive(Debug)]
struct Measurement {
    cpu_ms: f64,
    cpu_percent: f64,
    frames: usize,
    frames_per_second: f64,
    draw_ms: f64,
    tty_bytes: u64,
}

#[derive(Deserialize)]
struct FrameRecord {
    input_events: Vec<String>,
    draw_start_us: u64,
    draw_end_us: u64,
}

fn main() -> Result<()> {
    ensure!(
        cfg!(target_os = "linux"),
        "measure-cpu is supported only on Linux"
    );
    let root = workspace_root()?;
    let binary = root.join("target/release/diffo");
    let fixture = root.join("crates/diffo-core/fixtures/repository-state.ron");
    ensure!(
        binary.is_file(),
        "release binary is missing: {}",
        binary.display()
    );

    let ticks_per_second = clock_ticks_per_second()?;
    println!("Diffo release CPU measurement: 3 warmups, 5 samples, 5 seconds/sample");
    println!("scenario run cpu_ms cpu_pct frames fps draw_ms tty_bytes");
    for scenario in Scenario::ALL {
        for _ in 0..WARMUP_RUNS {
            measure_once(scenario, &binary, &fixture, ticks_per_second)?;
        }
        let mut measurements = Vec::with_capacity(MEASURED_RUNS);
        for run in 1..=MEASURED_RUNS {
            let measurement = measure_once(scenario, &binary, &fixture, ticks_per_second)?;
            print_measurement(scenario.name(), &run.to_string(), &measurement);
            measurements.push(measurement);
        }
        print_measurement(scenario.name(), "median", &median(&measurements));
    }
    Ok(())
}

#[allow(clippy::cast_precision_loss)]
fn measure_once(
    scenario: Scenario,
    binary: &Path,
    fixture: &Path,
    ticks_per_second: u64,
) -> Result<Measurement> {
    let temporary = tempfile::tempdir().context("create measurement directory")?;
    let trace = temporary.path().join("frames.ron");
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 30,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("open measurement PTY")?;
    let mut reader = pair.master.try_clone_reader().context("clone PTY reader")?;
    let mut writer = pair.master.take_writer().context("open PTY writer")?;
    let output_bytes = Arc::new(AtomicU64::new(0));
    let reader_bytes = Arc::clone(&output_bytes);
    let reader_thread = thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 {
                break;
            }
            reader_bytes.fetch_add(count as u64, Ordering::Relaxed);
        }
    });

    let mut command = CommandBuilder::new(binary.as_os_str());
    command.cwd(temporary.path().as_os_str());
    command.env("TERM", "xterm-256color");
    command.env("DIFFO_MOCK_FILE", fixture.as_os_str());
    command.env("DIFFO_TRACE_FRAMES", trace.as_os_str());
    let mut child = pair
        .slave
        .spawn_command(command)
        .context("launch release Diffo binary")?;
    drop(pair.slave);
    let pid = child.process_id().context("Diffo process has no PID")?;

    thread::sleep(SETTLE_TIME);
    ensure!(
        child.try_wait().context("poll Diffo")?.is_none(),
        "Diffo exited during startup"
    );
    let cpu_before = process_cpu_ticks(pid)?;
    let bytes_before = output_bytes.load(Ordering::Relaxed);
    let started = Instant::now();
    let mut sent_events = 0_usize;
    while started.elapsed() < SAMPLE_TIME {
        if matches!(scenario, Scenario::Scroll) {
            // SGR mouse-wheel down over the diff pane.
            writer
                .write_all(b"\x1b[<65;75;10M")
                .context("send scroll event")?;
            writer.flush().context("flush scroll event")?;
            sent_events += 1;
        }
        thread::sleep(Duration::from_millis(16));
    }
    let elapsed = started.elapsed();
    let cpu_after = process_cpu_ticks(pid)?;
    let tty_bytes = output_bytes
        .load(Ordering::Relaxed)
        .saturating_sub(bytes_before);

    writer.write_all(b"q").context("quit Diffo")?;
    writer.flush().context("flush quit")?;
    let status = child.wait().context("wait for Diffo")?;
    ensure!(status.success(), "Diffo exited unsuccessfully: {status:?}");
    drop(writer);
    reader_thread
        .join()
        .map_err(|_| anyhow::anyhow!("PTY reader panicked"))?;

    let (frames, input_events, draw_us) = trace_window(&trace, elapsed)?;
    match scenario {
        Scenario::Idle => ensure!(input_events == 0, "idle trace contained input events"),
        Scenario::Scroll => ensure!(
            input_events > 0 && sent_events > 0,
            "scroll workload did not reach Diffo"
        ),
    }
    let cpu_ticks = cpu_after.saturating_sub(cpu_before);
    let cpu_seconds = cpu_ticks as f64 / ticks_per_second as f64;
    let wall_seconds = elapsed.as_secs_f64();
    Ok(Measurement {
        cpu_ms: cpu_seconds * 1000.0,
        cpu_percent: cpu_seconds / wall_seconds * 100.0,
        frames,
        frames_per_second: frames as f64 / wall_seconds,
        draw_ms: draw_us as f64 / 1000.0,
        tty_bytes,
    })
}

fn trace_window(path: &Path, duration: Duration) -> Result<(usize, usize, u64)> {
    let contents = fs::read_to_string(path).context("read frame trace")?;
    let mut frames: Vec<FrameRecord> = contents
        .lines()
        .map(ron::from_str)
        .collect::<std::result::Result<_, _>>()
        .context("parse frame trace")?;
    frames.retain(|frame| {
        !frame
            .input_events
            .iter()
            .any(|event| event.contains("Char('q')"))
    });
    let end = frames.last().context("frame trace is empty")?.draw_end_us;
    let start = end.saturating_sub(u64::try_from(duration.as_micros()).unwrap_or(u64::MAX));
    let selected: Vec<_> = frames
        .iter()
        .filter(|frame| frame.draw_end_us >= start && frame.draw_end_us <= end)
        .collect();
    let input_events = selected.iter().map(|frame| frame.input_events.len()).sum();
    let draw_us = selected
        .iter()
        .map(|frame| frame.draw_end_us.saturating_sub(frame.draw_start_us))
        .sum();
    Ok((selected.len(), input_events, draw_us))
}

fn process_cpu_ticks(pid: u32) -> Result<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).context("read process CPU stat")?;
    let after_name = stat.rsplit_once(')').context("parse process CPU stat")?.1;
    let fields: Vec<_> = after_name.split_whitespace().collect();
    let user = fields
        .get(11)
        .context("missing user CPU field")?
        .parse::<u64>()?;
    let system = fields
        .get(12)
        .context("missing system CPU field")?
        .parse::<u64>()?;
    Ok(user.saturating_add(system))
}

fn clock_ticks_per_second() -> Result<u64> {
    let output = Command::new("getconf")
        .arg("CLK_TCK")
        .output()
        .context("run getconf CLK_TCK")?;
    ensure!(output.status.success(), "getconf CLK_TCK failed");
    String::from_utf8(output.stdout)?
        .trim()
        .parse()
        .context("parse CLK_TCK")
}

fn workspace_root() -> Result<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .context("locate workspace root")?;
    Ok(root.to_path_buf())
}

fn median(measurements: &[Measurement]) -> Measurement {
    fn middle(mut values: Vec<f64>) -> f64 {
        values.sort_by(f64::total_cmp);
        values[values.len() / 2]
    }
    fn middle_usize(mut values: Vec<usize>) -> usize {
        values.sort_unstable();
        values[values.len() / 2]
    }
    fn middle_u64(mut values: Vec<u64>) -> u64 {
        values.sort_unstable();
        values[values.len() / 2]
    }
    Measurement {
        cpu_ms: middle(measurements.iter().map(|item| item.cpu_ms).collect()),
        cpu_percent: middle(measurements.iter().map(|item| item.cpu_percent).collect()),
        frames: middle_usize(measurements.iter().map(|item| item.frames).collect()),
        frames_per_second: middle(
            measurements
                .iter()
                .map(|item| item.frames_per_second)
                .collect(),
        ),
        draw_ms: middle(measurements.iter().map(|item| item.draw_ms).collect()),
        tty_bytes: middle_u64(measurements.iter().map(|item| item.tty_bytes).collect()),
    }
}

fn print_measurement(scenario: &str, run: &str, measurement: &Measurement) {
    println!(
        "{scenario:<8} {run:>6} {:>7.1} {:>7.2} {:>6} {:>6.1} {:>8.1} {:>9}",
        measurement.cpu_ms,
        measurement.cpu_percent,
        measurement.frames,
        measurement.frames_per_second,
        measurement.draw_ms,
        measurement.tty_bytes,
    );
}
