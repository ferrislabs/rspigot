use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Local};
use flate2::Compression;
use flate2::write::GzEncoder;
use tracing_subscriber::fmt::MakeWriter;

pub const LATEST: &str = "latest.log";

pub fn archive_name(logs: &Path, date: &str) -> PathBuf {
    (1..)
        .map(|n| logs.join(format!("{date}-{n}.log.gz")))
        .find(|path| !path.exists())
        .unwrap_or_else(|| logs.join(format!("{date}-1.log.gz")))
}

pub fn rotate(logs: &Path) -> io::Result<Option<PathBuf>> {
    let latest = logs.join(LATEST);
    let Ok(metadata) = fs::metadata(&latest) else {
        return Ok(None);
    };
    if metadata.len() == 0 {
        fs::remove_file(&latest)?;
        return Ok(None);
    }
    let modified: DateTime<Local> = metadata.modified()?.into();
    let target = archive_name(logs, &modified.format("%Y-%m-%d").to_string());
    let mut source = File::open(&latest)?;
    let mut encoder = GzEncoder::new(File::create(&target)?, Compression::default());
    io::copy(&mut source, &mut encoder)?;
    encoder.finish()?.flush()?;
    fs::remove_file(&latest)?;
    Ok(Some(target))
}

#[derive(Clone)]
pub struct LatestLog {
    file: Arc<Mutex<File>>,
}

impl LatestLog {
    pub fn open(root: &Path) -> io::Result<Self> {
        let logs = root.join("logs");
        fs::create_dir_all(&logs)?;
        rotate(&logs)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(logs.join(LATEST))?;
        Ok(Self {
            file: Arc::new(Mutex::new(file)),
        })
    }
}

pub struct LatestWriter {
    file: Arc<Mutex<File>>,
}

impl Write for LatestWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.file.lock() {
            Ok(mut file) => file.write(buf),
            Err(_) => Ok(buf.len()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.file.lock() {
            Ok(mut file) => file.flush(),
            Err(_) => Ok(()),
        }
    }
}

impl<'a> MakeWriter<'a> for LatestLog {
    type Writer = LatestWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LatestWriter {
            file: Arc::clone(&self.file),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    fn unzip(path: &Path) -> String {
        let mut text = String::new();
        flate2::read::GzDecoder::new(File::open(path).unwrap())
            .read_to_string(&mut text)
            .unwrap();
        text
    }

    #[test]
    fn rotating_compresses_the_previous_latest_log() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(LATEST), "[10:00:00] [main/INFO]: hello\n").unwrap();
        let archive = rotate(dir.path()).unwrap().unwrap();
        assert!(!dir.path().join(LATEST).exists());
        assert_eq!(unzip(&archive), "[10:00:00] [main/INFO]: hello\n");
        let name = archive.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.ends_with("-1.log.gz"), "{name}");
    }

    #[test]
    fn the_archive_counter_skips_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("2026-11-16-1.log.gz"), b"x").unwrap();
        fs::write(dir.path().join("2026-11-16-2.log.gz"), b"x").unwrap();
        assert_eq!(
            archive_name(dir.path(), "2026-11-16"),
            dir.path().join("2026-11-16-3.log.gz")
        );
    }

    #[test]
    fn an_empty_latest_log_is_dropped_without_an_archive() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(LATEST), "").unwrap();
        assert_eq!(rotate(dir.path()).unwrap(), None);
        assert!(!dir.path().join(LATEST).exists());
    }

    #[test]
    fn opening_rotates_then_appends_to_a_fresh_latest_log() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("logs")).unwrap();
        fs::write(dir.path().join("logs").join(LATEST), "old\n").unwrap();
        let log = LatestLog::open(dir.path()).unwrap();
        log.make_writer().write_all(b"new\n").unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("logs").join(LATEST)).unwrap(),
            "new\n"
        );
        let archives = fs::read_dir(dir.path().join("logs")).unwrap().count();
        assert_eq!(archives, 2);
    }
}
