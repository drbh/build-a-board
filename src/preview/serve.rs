//! `bb preview --watch`: rebuild on every save and serve the result to a
//! browser tab that reloads itself, highlighting what each edit changed.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context as _, Result};
use serde_json::json;

use bb_core::preview::Scene;
use bb_core::preview::svg::{self, Status};

use super::{open, scene};
use crate::board::BoardFile;
use crate::kicad::Libraries;

/// What the page polls for.
#[derive(Default)]
struct Shared {
    version: u64,
    page: String,
    svg: String,
    status: String,
    changes: Option<String>,
    error: Option<String>,
}

pub struct WatchOptions {
    pub port: u16,
    pub routed: bool,
    pub open: bool,
}

/// Serve a live preview of the board at `path`, rebuilding when its sources change.
pub fn watch(path: &Path, footprints: &Path, options: &WatchOptions) -> Result<()> {
    let shared = Arc::new(Mutex::new(Shared::default()));
    let (listener, port) = bind(options.port)?;
    let url = format!("http://127.0.0.1:{port}/");
    {
        let shared = shared.clone();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let shared = shared.clone();
                thread::spawn(move || {
                    let _ = respond(stream, &shared);
                });
            }
        });
    }

    let dir =
        if path.is_dir() { path.to_path_buf() } else { path.parent().map_or_else(|| ".".into(), Path::to_path_buf) };
    let libraries = Libraries::new(footprints);
    let mut previous: Option<Scene> = None;
    // The scene before the last visible change.
    let mut baseline: Option<Scene> = None;
    let mut seen = None;
    let mut opened = false;
    println!("Previewing {} at {url} (Ctrl-C to stop)", dir.display());
    loop {
        let mut stamp = fingerprint(&dir);
        if seen.as_ref() != Some(&stamp) {
            // Let an editor finish writing: wait until the files stop changing.
            loop {
                thread::sleep(Duration::from_millis(150));
                let settled = fingerprint(&dir);
                if settled == stamp {
                    break;
                }
                stamp = settled;
            }
            seen = Some(stamp);
            let started = Instant::now();
            match build(path, footprints, &libraries, options.routed) {
                Ok(scene) => {
                    let status = Status::built(started.elapsed());
                    let (mut svg, mut changes) = svg::svg(&scene, previous.as_ref());
                    // A rebuild that changes nothing (a second save) keeps the
                    // last edit's highlights rather than clearing them.
                    if changes.as_deref() == Some(svg::NO_CHANGE) {
                        if let Some(baseline) = &baseline {
                            (svg, changes) = svg::svg(&scene, Some(baseline));
                        }
                    } else {
                        baseline = previous.take();
                    }
                    let status_line = svg::status_line(&scene, &status);
                    println!("{status_line}{}", changes.as_deref().map(|c| format!(" · {c}")).unwrap_or_default());
                    super::report(&scene);
                    let mut state = shared.lock().expect("preview state");
                    state.version += 1;
                    state.page = svg::page_with(&scene, &svg, &status, true);
                    state.svg = svg;
                    state.status = status_line;
                    state.changes = changes;
                    state.error = None;
                    previous = Some(scene);
                }
                Err(err) => {
                    let message = format!("{err:#}");
                    eprintln!("error: {message}");
                    let mut state = shared.lock().expect("preview state");
                    if state.page.is_empty() {
                        state.page = format!(
                            "<!doctype html><title>bb preview</title><pre>{}</pre>",
                            message.replace('<', "&lt;")
                        );
                    }
                    state.error = Some(message);
                }
            }
            if options.open && !opened {
                open(&url);
                opened = true;
            }
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn build(path: &Path, footprints: &Path, libraries: &Libraries, routed: bool) -> Result<Scene> {
    let board = BoardFile::load(path, footprints)?;
    for warning in &board.warnings {
        eprintln!("warning: {warning}");
    }
    let pcb = board.dir.join(&board.project.kicad_dir).join(format!("{}.kicad_pcb", board.project.name));
    scene(&board, libraries, routed.then_some(pcb.as_path()))
}

/// Bind the first free port from `port` up.
fn bind(port: u16) -> Result<(TcpListener, u16)> {
    for candidate in port..port.saturating_add(20) {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", candidate)) {
            return Ok((listener, candidate));
        }
    }
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| {
            let port = l.local_addr()?.port();
            Ok((l, port))
        })
        .context("binding the preview server")
}

fn respond(stream: TcpStream, shared: &Mutex<Shared>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    // Drain the headers; nothing in them matters here.
    let mut line = String::new();
    while reader.read_line(&mut line)? > 2 {
        line.clear();
    }
    let target = request.split_whitespace().nth(1).unwrap_or("/");
    let (kind, body) = {
        let state = shared.lock().expect("preview state");
        match target.split('?').next().unwrap_or("/") {
            "/" => ("text/html; charset=utf-8", state.page.clone()),
            "/svg" => ("image/svg+xml", state.svg.clone()),
            "/state" => (
                "application/json",
                json!({
                    "version": state.version,
                    "status": state.status,
                    "changes": state.changes,
                    "error": state.error,
                })
                .to_string(),
            ),
            _ => ("text/plain", String::new()),
        }
    };
    let status = if body.is_empty() && kind == "text/plain" { "404 Not Found" } else { "200 OK" };
    let mut stream = stream;
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body.as_bytes())
}

/// Modification times and sizes of the board's sources: everything in its
/// directory except build output.
fn fingerprint(dir: &Path) -> Vec<(PathBuf, SystemTime, u64)> {
    let mut files: Vec<_> = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_entry(|e| !matches!(e.file_name().to_str(), Some("target" | "output" | ".git")))
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            Some((e.into_path(), meta.modified().ok()?, meta.len()))
        })
        .collect();
    files.sort();
    files
}
