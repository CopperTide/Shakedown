//! The example project (the project crate's `larkspur` fixture): a copy of it in a temporary
//! folder, and an app that opened that copy.
use std::path::{Path, PathBuf};

use super::AppFx;

/// The example project folder the project crate's tests use.
pub fn project_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../project/tests/fixtures/larkspur")
}

/// Copies the folder `from` into `to`, recursively.
pub fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A copy of the example project, removed when dropped.
pub fn project_copy() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&project_fixture(), dir.path());
    dir
}

/// An idle app that opened its own copy of the example project. It derefs to the app; the copy
/// is removed after the app is dropped.
pub struct ProjectFx {
    pub fx: AppFx,
    pub dir: tempfile::TempDir,
}

impl std::ops::Deref for ProjectFx {
    type Target = AppFx;

    fn deref(&self) -> &AppFx {
        &self.fx
    }
}

/// [`AppFx::idle`] on a fresh copy of the example project.
pub async fn project_app(kind: &str, tag: &str) -> ProjectFx {
    let dir = project_copy();
    let fx = AppFx::idle(kind, tag).project(dir.path()).start().await;
    ProjectFx { fx, dir }
}
