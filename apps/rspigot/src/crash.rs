use std::backtrace::Backtrace;
use std::fs;
use std::io;
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, TimeZone};

pub fn file_name<Tz: TimeZone>(now: &DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    format!("crash-{}-server.txt", now.format("%Y-%m-%d_%H.%M.%S"))
}

pub fn render<Tz: TimeZone>(
    now: &DateTime<Tz>,
    description: &str,
    thread: &str,
    message: &str,
    location: &str,
    backtrace: &str,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let mut out = String::new();
    out.push_str("---- Minecraft Crash Report ----\n");
    out.push_str("// Don't do that.\n\n");
    out.push_str(&format!("Time: {}\n", now.format("%Y-%m-%d %H:%M:%S %z")));
    out.push_str(&format!("Description: {description}\n\n"));
    out.push_str(&format!("panic in thread '{thread}': {message}\n"));
    out.push_str(&format!("\tat {location}\n"));
    for line in backtrace.lines() {
        out.push_str(&format!("\t{}\n", line.trim_end()));
    }
    out.push_str("\nA detailed walkthrough of the error, its code path and all known details is as follows:\n");
    out.push_str(&"-".repeat(87));
    out.push_str("\n\n-- System Details --\nDetails:\n");
    out.push_str(&format!(
        "\tMinecraft Version: {}\n",
        rspigot_data::VERSION_NAME
    ));
    out.push_str(&format!(
        "\tServer Software: rspigot {}\n",
        env!("CARGO_PKG_VERSION")
    ));
    out.push_str(&format!(
        "\tOperating System: {} ({})\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    out.push_str(&format!(
        "\tAvailable Processors: {}\n",
        std::thread::available_parallelism().map_or(1, usize::from)
    ));
    out
}

pub fn write(root: &Path, report: &str, now: &DateTime<Local>) -> io::Result<PathBuf> {
    let dir = root.join("crash-reports");
    fs::create_dir_all(&dir)?;
    let path = dir.join(file_name(now));
    fs::write(&path, report)?;
    Ok(path)
}

fn payload(info: &PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "Box<dyn Any>".to_owned())
}

pub fn install(root: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let now = Local::now();
        let current = std::thread::current();
        let location = info
            .location()
            .map_or_else(|| "unknown".to_owned(), ToString::to_string);
        let report = render(
            &now,
            "Exception in server tick loop",
            current.name().unwrap_or("unnamed"),
            &payload(info),
            &location,
            &Backtrace::force_capture().to_string(),
        );
        match write(&root, &report, &now) {
            Ok(path) => tracing::error!("This crash report has been saved to: {}", path.display()),
            Err(error) => tracing::error!(%error, "could not save the crash report"),
        }
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, TimeZone};

    use super::*;

    fn moment() -> DateTime<FixedOffset> {
        FixedOffset::east_opt(3600)
            .unwrap()
            .with_ymd_and_hms(2026, 11, 16, 15, 36, 21)
            .unwrap()
    }

    #[test]
    fn the_file_name_follows_the_vanilla_pattern() {
        assert_eq!(file_name(&moment()), "crash-2026-11-16_15.36.21-server.txt");
    }

    #[test]
    fn the_report_has_the_vanilla_sections() {
        let text = render(
            &moment(),
            "Ticking entity",
            "main",
            "boom",
            "a.rs:1:2",
            "0: f\n1: g",
        );
        assert!(text.starts_with("---- Minecraft Crash Report ----\n// "));
        assert!(text.contains("\nTime: 2026-11-16 15:36:21 +0100\n"));
        assert!(text.contains("Description: Ticking entity\n"));
        assert!(text.contains("panic in thread 'main': boom\n\tat a.rs:1:2\n\t0: f\n\t1: g\n"));
        assert!(text.contains("A detailed walkthrough of the error"));
        assert!(text.contains("-- System Details --\nDetails:\n\tMinecraft Version: "));
    }

    #[test]
    fn writing_creates_the_crash_reports_directory() {
        let dir = tempfile::tempdir().unwrap();
        let now = Local::now();
        let path = write(dir.path(), "report", &now).unwrap();
        assert_eq!(path.parent().unwrap(), dir.path().join("crash-reports"));
        assert_eq!(fs::read_to_string(path).unwrap(), "report");
    }
}
