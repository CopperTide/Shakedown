//! A world's data files, in its folder (`worlds/<id>/`) of the project's store (a folder on disk,
//! or hosted Shakedown's database): list them, read one, upload one (JSON, CSV, YAML or text, at
//! most [`world_data::MAX_FILE`] on every backend), set one value of a JSON file, or delete one.
//! Paths are relative to the world's folder; anything that leaves it (`..`, an absolute path, a
//! symlink pointing out) is refused with 400, and `world.yaml` and the view files are the world
//! editor's, not data.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use shakedown_project::format::Kind;
use shakedown_project::store::{check_path, Store};
use shakedown_project::{Id, ProjectError};

use super::{flag, input, schema, Access, OpCtx, OpError, OpFuture, Operation};
use crate::world_data;

/// Most files a listing shows.
const MAX_FILES: usize = 500;
/// Most bytes of a file's text returned (with `full`, up to [`world_data::MAX_FILE`]).
const MAX_BYTES: u64 = 1 << 20;
/// The data files a world takes, by extension.
const KINDS: [&str; 7] = ["json", "csv", "yaml", "yml", "txt", "md", "tsv"];

pub(super) fn ops() -> Vec<Operation> {
    let path = "/api/project/worlds/{id}/data";
    let target = Some(Kind::World);
    vec![
        Operation {
            name: "worlds.data",
            area: "Worlds",
            summary: "Lists a world's data files, or reads one (text up to 1 MiB, or 10 MiB with full; binary files only their size)",
            access: Access::Read,
            method: "GET",
            path,
            target,
            input_schema: schema::<DataInput>,
            run: data,
        },
        Operation {
            name: "worlds.data_put",
            area: "Worlds",
            summary: "Uploads a data file into a world's folder (JSON, CSV, YAML or text, at most 10 MiB; base64 or text), replacing one there, or sets one value of a JSON data file at a JSON pointer",
            access: Access::Edit,
            method: "POST",
            path,
            target,
            input_schema: schema::<PutInput>,
            run: put,
        },
        Operation {
            name: "worlds.data_delete",
            area: "Worlds",
            summary: "Deletes a data file from a world's folder",
            access: Access::Delete,
            method: "DELETE",
            path,
            target,
            input_schema: schema::<PathInput>,
            run: delete,
        },
    ]
}

/// What `worlds.data` takes.
#[derive(Deserialize, JsonSchema)]
struct DataInput {
    /// The world's id: lowercase letters, digits and dashes.
    id: String,
    /// A file in the world's data folder, e.g. `suppliers.csv`; missing lists the files instead.
    #[serde(default)]
    path: Option<String>,
    /// Read the whole file, up to 10 MiB, rather than its first MiB.
    #[serde(default, deserialize_with = "flag")]
    full: bool,
}

/// What `worlds.data_put` takes: the file's bytes (`data`, base64, or `text`), or one value to
/// set in a JSON file (`pointer` and `value`).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PutInput {
    /// The world's id.
    id: String,
    /// The file, relative to the world's folder, e.g. `data/db.json`.
    path: String,
    /// The file's bytes, base64.
    #[serde(default)]
    data: Option<String>,
    /// The file's text.
    #[serde(default)]
    text: Option<String>,
    /// A JSON pointer into the JSON file at `path`, e.g. `/users/0/name`, whose value `value`
    /// replaces.
    #[serde(default)]
    pointer: Option<String>,
    /// The value set at `pointer`.
    #[serde(default)]
    value: Option<Value>,
}

/// One data file of a world.
#[derive(Deserialize, JsonSchema)]
struct PathInput {
    /// The world's id.
    id: String,
    /// The file, relative to the world's folder.
    path: String,
}

fn data<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let DataInput { id, path, full } = input(value)?;
        let (id, project) = (Id::parse(&id)?, ctx.project.current().await?);
        if project.get(Kind::World, &id).is_none() {
            return Err(ProjectError::not_found(Kind::World, &id).into());
        }
        let folder = format!("{}/{id}", Kind::World.dir());
        match path {
            None => Ok(list(&**project.store(), &folder)),
            Some(rel) => {
                let max = if full {
                    world_data::MAX_FILE
                } else {
                    MAX_BYTES
                };
                read(&**project.store(), &folder, &rel, max)
            }
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
fn read(store: &dyn Store, folder: &str, rel: &str, max: u64) -> Result<Value, OpError> {
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
    let (bytes, size) = match store.read_head(folder, &plain, max) {
        Ok(read) => read,
        Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => return Err(outside()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(not_found()),
        Err(_) if store.is_dir(&format!("{folder}/{plain}")) => return Err(not_found()),
        Err(e) => return Err(OpError::internal(e.into())),
    };
    let truncated = size > max;
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

/// `rel` as a data file of the world `id`: its path in the store, or 400 for one outside the
/// world's folder, `world.yaml`, a view file or a kind of file worlds don't take.
fn data_path(
    world: &shakedown_project::format::WorldFile,
    id: &Id,
    rel: &str,
) -> Result<String, OpError> {
    let plain = rel.trim_start_matches("./");
    check_path(plain)
        .map_err(|_| OpError::bad_request(format!("{rel:?} is outside the world's folder")))?;
    if plain == "world.yaml" || world.views.iter().any(|v| v == plain) {
        return Err(OpError::bad_request(format!(
            "{plain} is the world's own file: edit it in the world's editor"
        )));
    }
    let ext = plain
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    if !KINDS.contains(&ext.as_str()) {
        return Err(OpError::bad_request(format!(
            "{plain}: a world's data is JSON, CSV, YAML or text ({})",
            KINDS.join(", ")
        )));
    }
    Ok(format!("{}/{id}/{plain}", Kind::World.dir()))
}

/// The world `id` of the project, or 404.
fn world_of<'p>(
    project: &'p shakedown_project::Project,
    id: &Id,
) -> Result<&'p shakedown_project::format::WorldFile, OpError> {
    project
        .body::<shakedown_project::format::WorldFile>(id)
        .ok_or_else(|| ProjectError::not_found(Kind::World, id).into())
}

/// The new bytes of the file: those given, or the JSON file at `at` with one value set.
fn new_bytes(store: &dyn Store, at: &str, put: PutInput) -> Result<Vec<u8>, OpError> {
    use base64::Engine;
    match (put.data, put.text, put.pointer) {
        (Some(data), None, None) => base64::engine::general_purpose::STANDARD
            .decode(data.trim())
            .map_err(|e| OpError::bad_request(format!("data is not base64: {e}"))),
        (None, Some(text), None) => Ok(text.into_bytes()),
        (None, None, Some(pointer)) => {
            set_value(store, at, &pointer, put.value.unwrap_or(Value::Null))
        }
        _ => Err(OpError::bad_request(
            "give one of data, text, or pointer with value",
        )),
    }
}

/// The JSON file at `at` with `value` at `pointer`, written back as it was laid out when it fits
/// on one line per record, else pretty.
fn set_value(store: &dyn Store, at: &str, pointer: &str, value: Value) -> Result<Vec<u8>, OpError> {
    let text = store.read_text(at).map_err(|_| {
        OpError::new(
            axum::http::StatusCode::NOT_FOUND,
            format!("no data file {at}"),
        )
    })?;
    let mut doc: Value = serde_json::from_str(&text)
        .map_err(|e| OpError::bad_request(format!("{at} is not JSON: {e}")))?;
    let slot = doc
        .pointer_mut(pointer)
        .ok_or_else(|| OpError::bad_request(format!("{at} has nothing at {pointer:?}")))?;
    *slot = value;
    let out = if text.trim_start().starts_with(['{', '[']) && text.lines().count() > 1 {
        serde_json::to_string_pretty(&doc)
    } else {
        serde_json::to_string(&doc)
    };
    Ok(out.map_err(|e| OpError::internal(e.into()))?.into_bytes())
}

/// Refuses bytes too large for a data file, or that are not what their extension says.
fn checked(path: &str, bytes: &[u8]) -> Result<(), OpError> {
    if bytes.len() as u64 > world_data::MAX_FILE {
        let mib = world_data::MAX_FILE >> 20;
        return Err(OpError::new(
            axum::http::StatusCode::PAYLOAD_TOO_LARGE,
            format!("{path} is larger than {mib} MiB, the most a world's data file holds"),
        ));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| OpError::bad_request(format!("{path} is not text (UTF-8)")))?;
    let ext = path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    let parsed = match ext.as_str() {
        "json" => serde_json::from_str::<Value>(text)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        "yaml" | "yml" => serde_yaml_ng::from_str::<serde_yaml_ng::Value>(text)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        _ => Ok(()),
    };
    parsed.map_err(|e| OpError::bad_request(format!("{path} does not parse: {e}")))
}

fn put<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let put: PutInput = input(value)?;
        let id = Id::parse(&put.id)?;
        let project = ctx.project.current().await?;
        let at = data_path(world_of(&project, &id)?, &id, &put.path)?;
        let replaces = project.store().is_file(&at);
        let bytes = new_bytes(&**project.store(), &at, put)?;
        checked(&at, &bytes)?;
        let size = bytes.len();
        let io = |source| ProjectError::Io {
            action: "writing",
            file: at.clone(),
            source,
        };
        ctx.project
            .change(|p| p.store().write(&at, &bytes).map_err(io))
            .await??;
        Ok(json!({ "path": at, "size": size, "replaced": replaces }))
    })
}

fn delete<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let PathInput { id, path } = input(value)?;
        let id = Id::parse(&id)?;
        let project = ctx.project.current().await?;
        let at = data_path(world_of(&project, &id)?, &id, &path)?;
        if !project.store().is_file(&at) {
            return Err(OpError::new(
                axum::http::StatusCode::NOT_FOUND,
                format!("no data file {path}"),
            ));
        }
        let io = |source| ProjectError::Io {
            action: "deleting",
            file: at.clone(),
            source,
        };
        ctx.project
            .change(|p| p.store().remove(&at).map_err(io))
            .await??;
        Ok(json!({ "deleted": at }))
    })
}
