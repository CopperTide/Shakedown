//! A world's view files: the `views:` of its `world.yaml`, read from the world's folder and parsed.
//! A file that is missing or doesn't parse is a problem of the world and is left out.

use std::path::Path;

use crate::format::{parse, Kind, ViewFile, WorldFile};
use crate::item::{ItemBody, Problem};
use crate::load::Items;
use crate::Id;

/// One view of a world: its id (the file's stem), its file and what it parsed to.
#[derive(Debug, Clone)]
pub struct WorldView {
    pub id: String,
    /// Relative to the project folder.
    pub file: String,
    pub view: Result<ViewFile, String>,
}

/// The views of `world` (whose id is `id`) in the project at `root`, in the file's order.
pub fn world_views(root: &Path, id: &Id, world: &WorldFile) -> Vec<WorldView> {
    let read = |rel: &String| {
        let file = format!("worlds/{id}/{rel}");
        let stem = Path::new(rel)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string());
        let view = match std::fs::read_to_string(root.join(&file)) {
            Ok(text) => parse::<ViewFile>(&file, &text).map_err(|e| e.to_string()),
            Err(e) => Err(format!("{file}: {e}")),
        };
        WorldView {
            id: stem.unwrap_or_default(),
            file,
            view,
        }
    };
    world.views.iter().map(read).collect()
}

/// A problem for each view file of a world in `items` that can't be read or parsed.
pub(crate) fn view_problems(root: &Path, items: &Items) -> Vec<Problem> {
    let worlds = items
        .iter()
        .filter_map(|((kind, id), item)| match &item.body {
            ItemBody::World(w) if *kind == Kind::World => Some((id, w)),
            _ => None,
        });
    let mut problems = vec![];
    for (id, world) in worlds {
        let views = world_views(root, id, world).into_iter().enumerate();
        problems.extend(views.filter_map(|(i, v)| {
            v.view.err().map(|message| Problem {
                kind: Some(Kind::World),
                id: Some(id.to_string()),
                pointer: format!("/views/{i}"),
                message,
            })
        }));
    }
    problems
}
