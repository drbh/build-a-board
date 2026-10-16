//! Release archives (deflated with zlib, so they match Python's `zipfile`).

use std::fs::{self, File};
use std::io;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Datelike, Local, Timelike};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// Zip files and directory trees (minus Python caches). Entries are named
/// relative to `root`, or by bare file name when `flatten` is set.
pub fn zip_files(destination: &Path, root: &Path, entries: &[PathBuf], flatten: bool) -> Result<()> {
    // Check everything first, so a missing input never leaves a partial archive.
    let missing: Vec<String> = entries
        .iter()
        .filter(|entry| !entry.exists())
        .map(|entry| entry.strip_prefix(root).unwrap_or(entry).display().to_string())
        .collect();
    if !missing.is_empty() {
        bail!("cannot write {}: missing {}", destination.display(), missing.join(", "));
    }
    match fs::remove_file(destination) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err.into()),
        _ => {}
    }
    let file = File::create(destination).with_context(|| format!("creating {}", destination.display()))?;
    let mut archive = ZipWriter::new(file);
    for entry in entries {
        let files = if entry.is_dir() { tree_files(entry) } else { vec![entry.clone()] };
        for path in files {
            let name = if flatten {
                PathBuf::from(path.file_name().unwrap_or_default())
            } else {
                path.strip_prefix(root)
                    .map_err(|_| anyhow!("{} is outside {}", path.display(), root.display()))?
                    .to_path_buf()
            };
            add_file(&mut archive, &path, &name.to_string_lossy())?;
        }
    }
    archive.finish()?;
    Ok(())
}

fn tree_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = WalkDir::new(dir)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .map(walkdir::DirEntry::into_path)
        .filter(|path| path.is_file())
        .filter(|path| !path.components().any(|c| c == Component::Normal("__pycache__".as_ref())))
        .filter(|path| path.extension().is_none_or(|ext| ext != "pyc"))
        .collect();
    files.sort();
    files
}

fn add_file(archive: &mut ZipWriter<File>, path: &Path, name: &str) -> Result<()> {
    let metadata = fs::metadata(path).with_context(|| format!("reading {}", path.display()))?;
    let mut options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    if let Ok(modified) = metadata.modified() {
        let local: DateTime<Local> = modified.into();
        let stamp = zip::DateTime::from_date_and_time(
            u16::try_from(local.year()).unwrap_or(1980),
            local.month() as u8,
            local.day() as u8,
            local.hour() as u8,
            local.minute() as u8,
            local.second() as u8,
        )
        .map_err(|_| anyhow!("{}: ZIP cannot store timestamps before 1980", path.display()))?;
        options = options.last_modified_time(stamp);
    }
    #[cfg(unix)]
    {
        options = options.unix_permissions(std::os::unix::fs::PermissionsExt::mode(&metadata.permissions()));
    }
    archive.start_file(name, options)?;
    io::copy(&mut File::open(path)?, archive)?;
    Ok(())
}

/// Copy contents, permissions, and timestamps (like `shutil.copy2`).
pub fn copy_with_times(source: &Path, destination: &Path) -> Result<()> {
    fs::copy(source, destination)
        .with_context(|| format!("copying {} to {}", source.display(), destination.display()))?;
    let metadata = fs::metadata(source)?;
    let mut times = fs::FileTimes::new().set_modified(metadata.modified()?);
    if let Ok(accessed) = metadata.accessed() {
        times = times.set_accessed(accessed);
    }
    File::options().write(true).open(destination)?.set_times(times)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_input_names_every_gap_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("present.txt"), "x").unwrap();
        let zip = dir.path().join("out.zip");
        let entries = ["present.txt", "Makefile", "docs"].map(|f| dir.path().join(f));
        let err = zip_files(&zip, dir.path(), &entries, false).unwrap_err().to_string();
        assert!(err.ends_with("missing Makefile, docs"), "{err}");
        assert!(!zip.exists());
    }
}
