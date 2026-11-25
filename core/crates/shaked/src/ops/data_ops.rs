//! Reading a world's data files (read-only): list the files in its data folder (`worlds/<id>/`), or read one.
//! Paths are relative to that folder; anything that leaves it (`..`, an absolute path, a symlink
//! pointing out) is refused with 400.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use shakedown_project::format::Kind;
use shakedown_project::store::{check_path, Store};
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
        let (id, project) = (Id::parse(&id)?, ctx.project.current().await?);
        if project.get(Kind::World, &id).is_none() {
            return Err(ProjectError::not_found(Kind::World, &id).into());
        }
        let folder = format!("{}/{id}", Kind::World.dir());
        match path {
            None => Ok(list(&**project.store(), &folder)),
            Some(rel) => read(&**project.store(), &folder, &rel),
        }
    })
}

/// The files under `folder` (relative path and size), sorted, up to [`MAX_FILES`].
fn list(store: &dyn Store, folder: &str) -> Value {
    let mut files = files_under(store, folder);
    files.sort();
    let truncated = files.len() > MAX_FILES;
    files.truncate(MAX_FILES);
    let files: Vec<Value> = files
        .into_iter()
        .map(|(path, size)| json!({ "path": path, "size": size }))
        .collect();
    json!({ "files": files, "truncated": truncated })
}

/// Every file under `folder`, by path in it, with its size; links are left out.
fn files_under(store: &dyn Store, folder: &str) -> Vec<(String, u64)> {
    let (mut files, mut todo) = (Vec::new(), vec![String::new()]);
    while let Some(rel) = todo.pop() {
        let join = |a: &str, b: &str| match (a.is_empty(), b.is_empty()) {
            (true, _) => b.to_string(),
            (_, true) => a.to_string(),
            _ => format!("{a}/{b}"),
        };
        for entry in store.list(&join(folder, &rel)).into_iter().flatten() {
            let path = join(&rel, &entry.name);
            match (entry.link, entry.dir) {
                (true, _) => {}
                (false, true) => todo.push(path),
                (false, false) => files.push((path, entry.size)),
            }
        }
    }
    files
}

/// `{path, size, text, truncated}` for a text file, `{path, size, binary: true}` otherwise; 400
/// for a path that leaves the folder, 404 for a missing file.
fn read(store: &dyn Store, folder: &str, rel: &str) -> Result<Value, OpError> {
    let plain = rel
        .split('/')
        .filter(|s| *s != ".")
        .collect::<Vec<_>>()
        .join("/");
    let outside = || OpError::bad_request(format!("{rel:?} is outside the data folder"));
    check_path(&plain).map_err(|_| outside())?;
    let not_found = || {
        OpError::new(
            axum::http::StatusCode::NOT_FOUND,
            format!("no data file {rel}"),
        )
    };
    let (bytes, size) = match store.read_head(folder, &plain, MAX_BYTES) {
        Ok(read) => read,
        Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => return Err(outside()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(not_found()),
        Err(_) if store.is_dir(&format!("{folder}/{plain}")) => return Err(not_found()),
        Err(e) => return Err(OpError::internal(e.into())),
    };
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
