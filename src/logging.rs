use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tracing_subscriber::{
    EnvFilter,
    fmt::{
        format::Writer,
        time::{FormatTime, SystemTime},
    },
};

pub(crate) struct Logging(Arc<Mutex<Output>>);

pub(crate) fn init() -> Logging {
    let output = Arc::new(Mutex::new(Output {
        line: Vec::new(),
        count: 0,
        reported: 0,
        last_report: Instant::now(),
    }));
    let writer = CoalescingWriter(output.clone());
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .without_time()
        .with_writer(move || writer.clone())
        .init();
    Logging(output)
}

impl Drop for Logging {
    fn drop(&mut self) {
        if let Ok(mut output) = self.0.lock() {
            let _ = output.flush_repeats();
        }
    }
}

#[derive(Clone)]
struct CoalescingWriter(Arc<Mutex<Output>>);

impl Write for CoalescingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        // tracing-subscriber writes each fully formatted event in one write_all call.
        let mut output = self.0.lock().unwrap();
        if bytes == output.line {
            output.count = output.count.saturating_add(1);
            if output.last_report.elapsed() >= Duration::from_secs(1) {
                output.flush_repeats()?;
            }
        } else {
            output.flush_repeats()?;
            write_line(bytes, 1)?;
            output.line.clear();
            output.line.extend_from_slice(bytes);
            output.count = 1;
            output.reported = 1;
            output.last_report = Instant::now();
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().unwrap().flush_repeats()?;
        io::stdout().flush()
    }
}

struct Output {
    line: Vec<u8>,
    count: u64,
    reported: u64,
    last_report: Instant,
}

impl Output {
    fn flush_repeats(&mut self) -> io::Result<()> {
        if self.count > self.reported {
            write_line(&self.line, self.count)?;
            self.reported = self.count;
            self.last_report = Instant::now();
        }
        Ok(())
    }
}

fn write_line(line: &[u8], count: u64) -> io::Result<()> {
    let mut timestamp = String::new();
    SystemTime
        .format_time(&mut Writer::new(&mut timestamp))
        .map_err(io::Error::other)?;
    let mut stdout = io::stdout().lock();
    write!(stdout, "{timestamp} ")?;
    stdout.write_all(line.strip_suffix(b"\n").unwrap_or(line))?;
    if count > 1 {
        write!(stdout, " (x{count})")?;
    }
    writeln!(stdout)
}
