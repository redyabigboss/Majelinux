use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::core::Result;
use crate::ffi::{Tm, localtime_r};

const LOG_BUFFER_LIMIT: usize = 8 * 1024;
const LOG_BATCH_LINES: usize = 8;
const LOG_MAX_DELAY: std::time::Duration = std::time::Duration::from_millis(100);

struct Sink {
    file: File,
    pending: Vec<u8>,
    pending_lines: usize,
    last_flush: Instant,
}

struct Reader {
    path: PathBuf,
    file: File,
    buffer: Vec<u8>,
}

static SINK: Mutex<Option<Sink>> = Mutex::new(None);
static READER: Mutex<Option<Reader>> = Mutex::new(None);
static HOOK: OnceLock<fn()> = OnceLock::new();
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

pub fn set_hook(hook: fn()) {
    let _ = HOOK.set(hook);
}

fn parts() -> (i32, i32, i32, i32, i32, i32) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut tm = Tm::default();

    let ok = unsafe { !localtime_r(&secs, &mut tm).is_null() };
    if !ok {
        return (0, 0, 0, 0, 0, 0);
    }
    (
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
    )
}

fn flush_sink(sink: &mut Sink) {
    if sink.pending.is_empty() {
        return;
    }
    let _ = sink.file.write_all(&sink.pending);
    sink.pending.clear();
    sink.pending_lines = 0;
    sink.last_flush = Instant::now();
}

pub fn flush() {
    if let Ok(mut guard) = SINK.lock()
        && let Some(sink) = guard.as_mut()
    {
        flush_sink(sink);
        let _ = sink.file.flush();
    }
}

pub fn shutdown() {
    SHUTTING_DOWN.store(true, Ordering::Release);
    flush();
}

pub fn start(dir: &Path, prefix: &str) -> Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let (year, month, day, hour, min, sec) = parts();
    let path = dir.join(format!(
        "{prefix}{year:04}-{month:02}-{day:02}_{hour:02}-{min:02}-{sec:02}.log"
    ));
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    if let Ok(mut guard) = SINK.lock() {
        if let Some(old) = guard.as_mut() {
            flush_sink(old);
        }
        *guard = Some(Sink {
            file,
            pending: Vec::with_capacity(LOG_BUFFER_LIMIT),
            pending_lines: 0,
            last_flush: Instant::now(),
        });
    }
    Ok(path)
}

pub fn line(message: &str) {
    if let Ok(mut guard) = SINK.lock()
        && let Some(sink) = guard.as_mut()
    {
        let (_, _, _, hour, min, sec) = parts();
        let _ = writeln!(sink.pending, "[{hour:02}:{min:02}:{sec:02}] {message}");
        sink.pending_lines += 1;
        if sink.pending.len() >= LOG_BUFFER_LIMIT
            || sink.pending_lines >= LOG_BATCH_LINES
            || sink.last_flush.elapsed() >= LOG_MAX_DELAY
            || SHUTTING_DOWN.load(Ordering::Acquire)
        {
            flush_sink(sink);
        }
    }
    if let Some(hook) = HOOK.get() {
        hook();
    }
}

pub fn cleanup(dir: &Path, extra: &[PathBuf]) {
    flush();
    let _ = fs::create_dir_all(dir);
    let now = SystemTime::now();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("log") {
                continue;
            }
            let fresh = entry
                .metadata()
                .ok()
                .and_then(|meta| meta.modified().ok())
                .and_then(|time| now.duration_since(time).ok())
                .map(|age| age.as_secs() < 600)
                .unwrap_or(false);
            if fresh {
                continue;
            }
            let _ = fs::remove_file(path);
        }
    }
    for path in extra {
        let _ = fs::remove_file(path);
    }
}

pub fn read_new(path: &Path, offset: u64) -> (String, u64) {
    flush();

    let Ok(mut guard) = READER.lock() else {
        return (String::new(), offset);
    };
    let needs_open = guard
        .as_ref()
        .is_none_or(|reader| reader.path.as_path() != path);
    if needs_open {
        let Ok(file) = File::open(path) else {
            *guard = None;
            return (String::new(), offset);
        };
        *guard = Some(Reader {
            path: path.to_path_buf(),
            file,
            buffer: Vec::with_capacity(4096),
        });
    }

    let Some(reader) = guard.as_mut() else {
        return (String::new(), offset);
    };
    let len = reader.file.metadata().map(|meta| meta.len()).unwrap_or(0);
    let mut start = offset;
    if start > len {
        start = 0;
    }
    if start == len {
        return (String::new(), len);
    }
    if reader.file.seek(SeekFrom::Start(start)).is_err() {
        return (String::new(), start);
    }
    reader.buffer.clear();
    if reader.file.read_to_end(&mut reader.buffer).is_err() {
        return (String::new(), start);
    }
    let valid = match std::str::from_utf8(&reader.buffer) {
        Ok(_) => reader.buffer.len(),
        Err(error) if error.error_len().is_none() => error.valid_up_to(),
        Err(_) => reader.buffer.len(),
    };
    (
        String::from_utf8_lossy(&reader.buffer[..valid]).into_owned(),
        start + valid as u64,
    )
}
