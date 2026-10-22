use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

const MAX_BYTES: u64 = 1024 * 1024;
const KEEP: usize = 3;

/// Size-capped log file with numbered rotation, so a chatty backend cannot fill the disk.
#[derive(Debug)]
pub(crate) struct RotatingLog {
    inner: Mutex<Inner>,
}

#[derive(Debug)]
struct Inner {
    path: PathBuf,
    file: File,
    written: u64,
}

impl RotatingLog {
    pub(crate) fn open(dir: &Path, name: &str) -> std::io::Result<Self> {
        fs::create_dir_all(dir)?;
        let path = dir.join(name);
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let written = file.metadata()?.len();
        Ok(Self {
            inner: Mutex::new(Inner {
                path,
                file,
                written,
            }),
        })
    }

    pub(crate) fn line(&self, text: &str) {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let bytes = text.len() as u64 + 1;
        if inner.written + bytes > MAX_BYTES {
            let _ = inner.rotate();
        }
        if writeln!(inner.file, "{text}").is_ok() {
            inner.written += bytes;
        }
    }
}

impl Inner {
    fn rotate(&mut self) -> std::io::Result<()> {
        for index in (1..KEEP).rev() {
            let from = self.numbered(index);
            if from.exists() {
                fs::rename(&from, self.numbered(index + 1))?;
            }
        }
        fs::rename(&self.path, self.numbered(1))?;
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        self.written = 0;
        Ok(())
    }

    fn numbered(&self, index: usize) -> PathBuf {
        let mut name = self.path.file_name().unwrap_or_default().to_os_string();
        name.push(format!(".{index}"));
        self.path.with_file_name(name)
    }
}
