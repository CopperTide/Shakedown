//! Reading a world's data files (read-only): list the files in its data folder (`worlds/<id>/`), or read one.
//! Paths are relative to that folder; anything that leaves it (`..`, an absolute path, a symlink
//! pointing out) is refused with 400.

use std::io::Read;
use std::path::{Component, Path, PathBuf};

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use shakedown_project::format::Kind;
use shakedown_project::{Id, ProjectError};

use super::{input, schema, Access, OpCtx, OpError, OpFuture, Operation};

/// Most files a listing shows.
const MAX_FILES: usize = 500;
/// Most bytes of a file's text returned.
const MAX_BYTES: u64 = 1 << 20;

pub(super) fn ops() -> Vec<Operation> {
    vec![Operation {
        name: "worlds.data",
        area: "Worlds",
        summary: "Lists a world's data files, or reads one (text up to 1 MiB; binary files only their size)",
        access: Access::Read,
        method: "GET",
        path: "/api/project/worlds/{id}/data",
        target: Some(Kind::World),
        input_schema: schema::<DataInput>,
        run: data,
    }]
}

/// What `worlds.data` takes.
#[derive(Deserialize, JsonSchema)]
struct DataInput {
    /// The world's id: lowercase letters, digits and dashes.
    id: String,
    /// A file in the world's data folder, e.g. `suppliers.csv`; missing lists the files instead.
    #[serde(default)]
    path: Option<String>,
}

fn data<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let DataInput { id, path } = input(value)?;
        let (id, project) = (Id::parse(&id)?, ctx.project.current()?);
        if project.get(Kind::World, &id).is_none() {
            return Err(ProjectError::not_found(Kind::World, &id).into());
        }
        let kept = project.kept_folder(Kind::World, &id);
        let folder = project.root.join(kept.unwrap_or_default());
        match path {
            None => list(&folder),
            Some(rel) => read(&folder, &rel),
        }
    })
}

/// The files under `folder` (relative path and size), sorted, up to [`MAX_FILES`].
fn list(folder: &Path) -> Result<Value, OpError> {
    let mut files = Vec::new();
    let mut todo = vec![folder.to_path_buf()];
    while let Some(dir) = todo.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                todo.push(entry.path());
            } else if kind.is_file() {
                let rel = entry.path().strip_prefix(folder).map(Path::to_path_buf);
                let size = entry.metadata().map_or(0, |m| m.len());
                files.extend(rel.map(|r| (r.to_string_lossy().replace('\\', "/"), size)));
            }
        }
    }
    files.sort();
    let truncated = files.len() > MAX_FILES;
    files.truncate(MAX_FILES);
    let files: Vec<Value> = files
        .into_iter()
        .map(|(path, size)| json!({ "path": path, "size": size }))
        .collect();
    Ok(json!({ "files": files, "truncated": truncated }))
}

/// The file `rel` under `folder`, canonical, or 400 if it leaves the folder, 404 if absent.
fn resolve(folder: &Path, rel: &str) -> Result<PathBuf, OpError> {
    let plain = Path::new(rel)
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if !plain {
        return Err(OpError::bad_request(format!(
            "{rel:?} is outside the data folder"
        )));
    }
    let not_found = || {
        OpError::new(
            axum::http::StatusCode::NOT_FOUND,
            format!("no data file {rel}"),
        )
    };
    let base = folder.canonicalize().map_err(|_| not_found())?;
    let file = base.join(rel).canonicalize().map_err(|_| not_found())?;
    if !file.starts_with(&base) {
        return Err(OpError::bad_request(format!(
            "{rel:?} is outside the data folder"
        )));
    }
    Ok(file)
}

/// `{path, size, text, truncated}` for a text file, `{path, size, binary: true}` otherwise.
fn read(folder: &Path, rel: &str) -> Result<Value, OpError> {
    let file = resolve(folder, rel)?;
    let io = |e: std::io::Error| OpError::internal(e.into());
    let size = file.metadata().map_err(io)?.len();
    let mut bytes = Vec::new();
    let opened = std::fs::File::open(&file).map_err(io)?;
    opened.take(MAX_BYTES).read_to_end(&mut bytes).map_err(io)?;
    let truncated = size > MAX_BYTES;
    match text_of(&bytes, truncated) {
        Some(text) => {
            Ok(json!({ "path": rel, "size": size, "text": text, "truncated": truncated }))
        }
        None => Ok(json!({ "path": rel, "size": size, "binary": true })),
    }
}

/// `bytes` as text, or `None` if they hold a NUL or are not UTF-8. A cut that splits the last
/// character of a truncated file is not an error.
fn text_of(bytes: &[u8], truncated: bool) -> Option<&str> {
    if bytes.contains(&0) {
        return None;
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => Some(text),
        Err(e) if truncated && e.error_len().is_none() => {
            std::str::from_utf8(&bytes[..e.valid_up_to()]).ok()
        }
        Err(_) => None,
    }
}
