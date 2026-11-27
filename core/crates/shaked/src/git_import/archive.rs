//! Unpacking an uploaded archive (`.zip`, `.tar.gz` or `.tgz`) as an import's checkout:
//! `<into>/<sha12>/`, named after the archive's SHA-256, which stands for the commit. Only
//! regular files and folders are unpacked (links are skipped), never outside the folder, and an
//! archive above [`MAX_BYTES`] unpacked or [`MAX_ENTRIES`] entries is refused. An archive whose
//! entries are all in one top folder (as GitHub's downloads are) is that folder.

use std::io::Read;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use super::fetch::{short, Fetched};
use super::ImportError;

/// Most bytes an archive may unpack to.
const MAX_BYTES: u64 = 2 << 30;
/// Most entries an archive may hold.
const MAX_ENTRIES: usize = 50_000;

/// The archive formats, by file name.
enum Format {
    Zip,
    TarGz,
}

fn format(name: &str) -> Result<Format, ImportError> {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".zip") {
        return Ok(Format::Zip);
    }
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        return Ok(Format::TarGz);
    }
    Err(ImportError::Bad(format!(
        "{name:?} is not a .zip, .tar.gz or .tgz archive"
    )))
}

/// The archive's name without its extension, for the default id.
pub fn stem(name: &str) -> &str {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let lower = base.to_ascii_lowercase();
    let cut = [".tar.gz", ".tgz", ".zip"]
        .iter()
        .find(|ext| lower.ends_with(*ext))
        .map_or(0, |ext| ext.len());
    &base[..base.len() - cut]
}

/// Unpacks the archive `name` (its `bytes`) into `<into>/<sha12>` (kept as it is when the same
/// archive was unpacked before).
pub fn unpack(name: &str, bytes: &[u8], into: &Path) -> Result<Fetched, ImportError> {
    let fmt = format(name)?;
    let commit: String = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let dir = into.join(short(&commit));
    if dir.is_dir() {
        return Ok(Fetched { dir, commit });
    }
    std::fs::create_dir_all(into).map_err(|e| ImportError::io("creating", into, e))?;
    let tmp = into.join(format!(
        ".unpack-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    ));
    let unpacked = unpack_into(fmt, bytes, &tmp).and_then(|()| {
        let top = single_folder(&tmp).unwrap_or_else(|| tmp.clone());
        std::fs::rename(&top, &dir).map_err(|e| ImportError::io("moving the archive to", &dir, e))
    });
    if tmp.exists() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    unpacked.map(|()| Fetched { dir, commit })
}

fn unpack_into(fmt: Format, bytes: &[u8], to: &Path) -> Result<(), ImportError> {
    std::fs::create_dir_all(to).map_err(|e| ImportError::io("creating", to, e))?;
    let mut budget = Budget::default();
    match fmt {
        Format::Zip => unzip(bytes, to, &mut budget),
        Format::TarGz => untar(bytes, to, &mut budget),
    }
}

/// What an archive has unpacked so far, against the limits.
#[derive(Default)]
struct Budget {
    entries: usize,
    bytes: u64,
}

impl Budget {
    fn take(&mut self, size: u64) -> Result<(), ImportError> {
        self.entries += 1;
        self.bytes = self.bytes.saturating_add(size);
        if self.entries > MAX_ENTRIES || self.bytes > MAX_BYTES {
            return Err(ImportError::Bad(format!(
                "the archive is too large: at most {MAX_ENTRIES} entries and {} GiB unpacked",
                MAX_BYTES >> 30
            )));
        }
        Ok(())
    }
}

/// `rel` as a path inside the archive's folder: relative, with no `..`; None for one that is not.
fn inside(rel: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for c in rel.components() {
        match c {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// Writes one file of the archive at `rel` under `to`, reading at most the size it declared.
fn write_file(to: &Path, rel: &Path, size: u64, from: &mut dyn Read) -> Result<(), ImportError> {
    let path = to.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ImportError::io("creating", parent, e))?;
    }
    let mut file =
        std::fs::File::create(&path).map_err(|e| ImportError::io("writing", &path, e))?;
    std::io::copy(&mut from.take(size), &mut file)
        .map_err(|e| ImportError::io("writing", &path, e))?;
    Ok(())
}

fn bad_archive(e: impl std::fmt::Display) -> ImportError {
    ImportError::Bad(format!("the archive can't be read: {e}"))
}

fn unzip(bytes: &[u8], to: &Path, budget: &mut Budget) -> Result<(), ImportError> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(bad_archive)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(bad_archive)?;
        budget.take(entry.size())?;
        let Some(rel) = entry.enclosed_name().as_deref().and_then(inside) else {
            continue;
        };
        if entry.is_dir() {
            let dir = to.join(&rel);
            std::fs::create_dir_all(&dir).map_err(|e| ImportError::io("creating", &dir, e))?;
        } else if entry.is_file() {
            let size = entry.size();
            write_file(to, &rel, size, &mut entry)?;
        }
    }
    Ok(())
}

fn untar(bytes: &[u8], to: &Path, budget: &mut Budget) -> Result<(), ImportError> {
    let gz = flate2::read::GzDecoder::new(bytes);
    let mut tar = tar::Archive::new(gz);
    for entry in tar.entries().map_err(bad_archive)? {
        let mut entry = entry.map_err(bad_archive)?;
        let size = entry.header().size().map_err(bad_archive)?;
        budget.take(size)?;
        let path = entry.path().map_err(bad_archive)?.into_owned();
        let Some(rel) = inside(&path) else {
            continue;
        };
        match entry.header().entry_type() {
            tar::EntryType::Directory => {
                let dir = to.join(&rel);
                std::fs::create_dir_all(&dir).map_err(|e| ImportError::io("creating", &dir, e))?;
            }
            tar::EntryType::Regular | tar::EntryType::Continuous => {
                write_file(to, &rel, size, &mut entry)?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// The one folder `dir` holds when it holds nothing else.
fn single_folder(dir: &Path) -> Option<PathBuf> {
    let mut entries = std::fs::read_dir(dir).ok()?.filter_map(Result::ok);
    let first = entries.next()?;
    let only = entries.next().is_none() && first.file_type().is_ok_and(|t| t.is_dir());
    only.then(|| first.path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tar_gz(files: &[(&str, &str)]) -> Vec<u8> {
        let gz = flate2::write::GzEncoder::new(vec![], flate2::Compression::fast());
        let mut tar = tar::Builder::new(gz);
        for (path, text) in files {
            let mut h = tar::Header::new_gnu();
            h.set_size(text.len() as u64);
            h.set_mode(0o644);
            h.set_entry_type(tar::EntryType::Regular);
            h.set_cksum();
            tar.append_data(&mut h, path, text.as_bytes()).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap()
    }

    fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(vec![]));
        let opts = zip::write::SimpleFileOptions::default();
        for (path, text) in files {
            zip.start_file(*path, opts).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn a_tar_gz_in_one_top_folder_is_that_folder_named_after_its_digest() {
        let into = tempfile::tempdir().unwrap();
        let bytes = tar_gz(&[
            ("repo-main/a/shakedown-package.toml", "x"),
            ("repo-main/b.txt", "y"),
        ]);
        let got = unpack("repo-main.tar.gz", &bytes, into.path()).unwrap();
        assert_eq!(got.commit.len(), 64);
        assert_eq!(got.dir, into.path().join(&got.commit[..12]));
        assert_eq!(std::fs::read_to_string(got.dir.join("b.txt")).unwrap(), "y");
        assert!(got.dir.join("a/shakedown-package.toml").is_file());
        let again = unpack("repo-main.tar.gz", &bytes, into.path()).unwrap();
        assert_eq!(again.dir, got.dir);
    }

    #[test]
    fn a_zip_unpacks_and_paths_that_leave_the_folder_are_skipped() {
        let into = tempfile::tempdir().unwrap();
        let bytes = zip_of(&[
            ("pkg/one.txt", "1"),
            ("../evil.txt", "no"),
            ("top.txt", "2"),
        ]);
        let got = unpack("pkg.zip", &bytes, into.path()).unwrap();
        assert!(got.dir.join("pkg/one.txt").is_file() && got.dir.join("top.txt").is_file());
        assert!(!into.path().join("evil.txt").exists());
        assert!(!into.path().parent().unwrap().join("evil.txt").exists());
    }

    #[test]
    fn other_files_are_refused_and_the_stem_drops_the_extension() {
        let into = tempfile::tempdir().unwrap();
        assert!(matches!(
            unpack("x.rar", b"", into.path()),
            Err(ImportError::Bad(_))
        ));
        assert_eq!(stem("/up/tau-world-main.tar.gz"), "tau-world-main");
        assert_eq!(stem("Desk.ZIP"), "Desk");
    }
}
