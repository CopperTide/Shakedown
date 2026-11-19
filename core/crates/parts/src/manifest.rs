//! `arena-package.toml` and the registry of installed packages.
use anyhow::{anyhow, bail, Context, Result};
use arena_part::proto::{valid_package_name, PROTOCOL};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Integration,
    Benchmark,
    /// A world builder: answers `templates`, `template`, `preview` and `generate`.
    Builder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Run {
    Command(Vec<String>),
    Image(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Needs {
    pub gpu_vram_gb: u32,
    pub services: Vec<String>,
    pub network: Option<String>,
}

/// Which built world a benchmark is: its `[world]` table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorldMeta {
    pub title: String,
    pub template: String,
    pub template_name: String,
}

#[derive(Debug, Clone)]
pub struct Manifest {
    pub kind: Kind,
    pub name: String,
    pub version: String,
    pub protocol: u32,
    pub run: Run,
    pub needs: Needs,
    /// The folder holding the manifest.
    pub dir: PathBuf,
    /// The `[[example]]` entries, `{tool, args}`.
    pub examples: Vec<Value>,
    pub config_schema: Value,
    /// Used only when chosen by name, never as the one benchmark installed.
    pub opt_in: bool,
    /// Set for a world the builder made.
    pub world: Option<WorldMeta>,
}

fn str_of(v: &Value, key: &str) -> Result<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("`{key}` is missing or not a string"))
}

impl Manifest {
    /// Reads `<dir>/arena-package.toml`.
    pub fn load(dir: &Path) -> Result<Manifest> {
        let file = dir.join("arena-package.toml");
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("reading {}", file.display()))?;
        Self::parse(&text, dir).with_context(|| format!("in {}", file.display()))
    }

    fn parse(text: &str, dir: &Path) -> Result<Manifest> {
        let v: Value = serde_json::to_value(text.parse::<toml::Table>()?)?;
        let kind = parse_kind(&v)?;
        let name = str_of(&v, "name")?;
        if !valid_package_name(&name) {
            bail!("package name {name:?} must match [a-z0-9-]{{1,40}}");
        }
        let protocol = v.get("protocol").and_then(Value::as_u64).unwrap_or(0) as u32;
        if protocol != PROTOCOL {
            bail!("protocol {protocol} is not supported, only {PROTOCOL}");
        }
        let run = parse_run(v.get("run").ok_or_else(|| anyhow!("`run` is missing"))?)?;
        Ok(Manifest {
            kind,
            name,
            version: str_of(&v, "version")?,
            protocol,
            run,
            needs: parse_needs(v.get("needs"))?,
            opt_in: v.get("opt_in").and_then(Value::as_bool).unwrap_or(false),
            dir: dir.to_path_buf(),
            examples: v
                .get("example")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            config_schema: v.get("config_schema").cloned().unwrap_or(Value::Null),
            world: parse_world(v.get("world"))?,
        })
    }
}

fn parse_kind(v: &Value) -> Result<Kind> {
    Ok(match str_of(v, "kind")?.as_str() {
        "integration" => Kind::Integration,
        "benchmark" => Kind::Benchmark,
        "builder" => Kind::Builder,
        k => bail!("kind must be integration, benchmark or builder, not {k:?}"),
    })
}

fn parse_world(world: Option<&Value>) -> Result<Option<WorldMeta>> {
    let Some(w) = world else {
        return Ok(None);
    };
    let field = |key: &str| str_of(w, key).map_err(|e| anyhow!("world.{key}: {e}"));
    Ok(Some(WorldMeta {
        title: field("title")?,
        template: field("template")?,
        template_name: field("template_name")?,
    }))
}

fn parse_run(run: &Value) -> Result<Run> {
    Ok(match (run.get("command"), run.get("image")) {
        (Some(c), None) => {
            let c: Vec<String> = serde_json::from_value(c.clone()).context("run.command")?;
            if c.is_empty() {
                bail!("run.command is empty");
            }
            Run::Command(c)
        }
        (None, Some(i)) => Run::Image(
            i.as_str()
                .ok_or_else(|| anyhow!("run.image must be a string"))?
                .into(),
        ),
        _ => bail!("run needs exactly one of command or image"),
    })
}

fn parse_needs(needs: Option<&Value>) -> Result<Needs> {
    let Some(n) = needs else {
        return Ok(Needs::default());
    };
    Ok(Needs {
        gpu_vram_gb: n.get("gpu_vram_gb").and_then(Value::as_u64).unwrap_or(0) as u32,
        services: match n.get("services") {
            Some(s) => serde_json::from_value(s.clone()).context("needs.services")?,
            None => vec![],
        },
        network: n.get("network").and_then(Value::as_str).map(str::to_string),
    })
}

/// The installed packages, by name.
#[derive(Debug, Default)]
pub struct Registry {
    by_name: BTreeMap<String, Manifest>,
}

impl Registry {
    /// Loads every `<root>/<folder>/arena-package.toml`. A missing root is skipped; a duplicate name is an error.
    pub fn scan(roots: &[PathBuf]) -> Result<Registry> {
        let mut by_name = BTreeMap::new();
        for root in roots {
            let Ok(rd) = std::fs::read_dir(root) else {
                continue;
            };
            let mut dirs: Vec<PathBuf> = rd.filter_map(|e| Some(e.ok()?.path())).collect();
            dirs.sort();
            for d in dirs
                .into_iter()
                .filter(|d| d.join("arena-package.toml").is_file())
            {
                let m = Manifest::load(&d)?;
                if let Some(old) = by_name.get(&m.name) {
                    let old: &Manifest = old;
                    bail!(
                        "package {:?} is installed twice: {} and {}",
                        m.name,
                        old.dir.display(),
                        d.display()
                    );
                }
                by_name.insert(m.name.clone(), m);
            }
        }
        Ok(Registry { by_name })
    }

    /// A registry of the one package in `dir`.
    pub fn from_dir(dir: &Path) -> Result<Registry> {
        let m = Manifest::load(dir)?;
        let mut by_name = BTreeMap::new();
        by_name.insert(m.name.clone(), m);
        Ok(Registry { by_name })
    }

    pub fn get(&self, name: &str) -> Option<&Manifest> {
        self.by_name.get(name)
    }

    fn of_kind(&self, kind: Kind) -> impl Iterator<Item = &Manifest> {
        self.by_name.values().filter(move |m| m.kind == kind)
    }

    pub fn benchmarks(&self) -> impl Iterator<Item = &Manifest> {
        self.of_kind(Kind::Benchmark)
    }

    pub fn integrations(&self) -> impl Iterator<Item = &Manifest> {
        self.of_kind(Kind::Integration)
    }

    pub fn builders(&self) -> impl Iterator<Item = &Manifest> {
        self.of_kind(Kind::Builder)
    }

    /// Adds `m`; refuses a name another package has.
    pub fn add(&mut self, m: Manifest) -> Result<()> {
        if self.by_name.contains_key(&m.name) {
            bail!("a package named {} is installed already", m.name);
        }
        self.by_name.insert(m.name.clone(), m);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BENCH: &str = r#"kind = "benchmark"
name = "acme"
version = "0.1.0"
protocol = 1
run = { command = ["x"] }"#;

    fn parse(text: &str) -> Manifest {
        Manifest::parse(text, Path::new("/p")).unwrap()
    }

    fn of(name: &str) -> Manifest {
        parse(&BENCH.replace("\"acme\"", &format!("{name:?}")))
    }

    #[test]
    fn a_builder_manifest_parses_and_a_world_table_is_read() {
        let m = parse(
            r#"kind = "builder"
name = "world-builder"
version = "0.1.0"
protocol = 1
run = { command = ["x"] }"#,
        );
        assert_eq!(m.kind, Kind::Builder);
        let w = parse(&format!(
            "{BENCH}\n[world]\ntitle = \"Acme\"\ntemplate = \"shop\"\ntemplate_name = \"Shop\""
        ));
        let want = WorldMeta {
            title: "Acme".into(),
            template: "shop".into(),
            template_name: "Shop".into(),
        };
        assert_eq!(w.world, Some(want));
        assert!(parse(BENCH).world.is_none());
    }

    #[test]
    fn a_world_table_without_its_title_is_refused() {
        let text = format!("{BENCH}\n[world]\ntemplate = \"shop\"\ntemplate_name = \"Shop\"");
        let e = Manifest::parse(&text, Path::new("/p")).unwrap_err();
        assert_eq!(e.to_string(), "world.title: `title` is missing or not a string");
    }

    #[test]
    fn builders_are_listed_apart_and_a_taken_name_is_refused() {
        let mut r = Registry::default();
        r.add(of("acme")).unwrap();
        let builder = parse(&BENCH.replace("benchmark", "builder").replace("acme", "wb"));
        r.add(builder).unwrap();
        let builders: Vec<&str> = r.builders().map(|m| &*m.name).collect();
        assert_eq!(builders, ["wb"]);
        assert_eq!(r.benchmarks().count(), 1);
        let e = r.add(of("acme")).unwrap_err();
        assert_eq!(e.to_string(), "a package named acme is installed already");
    }
}
