//! A benchmark's description kept as data beside its manifest: the file [`DESCRIBE_FILE`], what
//! its `describe` answers. Read where package programs don't run (a control box whose runs go to
//! sandboxes), so a benchmark's tasks and metadata are known without starting it.
use crate::manifest::Manifest;
use anyhow::{ensure, Context, Result};
use shakedown_part::proto::BenchDescribe;

/// A benchmark's description file, beside its `shakedown-package.toml`.
pub const DESCRIBE_FILE: &str = "describe.json";

/// A benchmark without a description file, asked for where its program doesn't run.
#[derive(Debug, thiserror::Error)]
#[error(
    "benchmark {0} has no description file (describe.json beside its manifest), and package \
     programs don't run on this shaked"
)]
pub struct NoDescription(pub String);

/// The description file of the benchmark `m`, which must name `m`.
pub fn read(m: &Manifest) -> Result<BenchDescribe> {
    let file = m.dir.join(DESCRIBE_FILE);
    if !file.is_file() {
        return Err(NoDescription(m.name.clone()).into());
    }
    let text =
        std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
    let d: BenchDescribe =
        serde_json::from_str(&text).with_context(|| format!("in {}", file.display()))?;
    let (named, file) = (&d.name, file.display());
    ensure!(*named == m.name, "{file} describes {named}, not {}", m.name);
    Ok(d)
}
