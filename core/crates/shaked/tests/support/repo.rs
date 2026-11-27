//! A git repo made in the test: a tiny project with one Python part run through uv (the toy
//! part), a benchmark over it and one world, for Import from git and runtime installs.

use std::path::Path;
use std::process::Command;

/// uv runs the Python part; without it the tests are skipped, saying why.
pub fn uv_found() -> bool {
    let found = Command::new("uv").arg("--version").output().is_ok();
    if !found {
        eprintln!("skipped: uv is not on PATH");
    }
    found
}

pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "git {args:?}: {err}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

pub fn write(root: &Path, file: &str, text: &str) {
    let path = root.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// The project files of the fixture repo, for packages named `<p>-...`.
pub fn project_files(p: &str) -> Vec<(&'static str, String)> {
    let setup = "format: shakedown-setup/1\nlabel: Solo\nteam:\n  kind: builtin\n  agents:\n    - {id: planner, role: You poke boxes., model: claude-sonnet, tools: [\"*\"]}\n";
    vec![
        ("shakedown.yaml", "format: shakedown-project/1\nname: Toy ops\ndefaults:\n  world: box\n".into()),
        ("worlds/box/world.yaml", format!("format: shakedown-world/3\nlabel: The box\npackage: {p}-bench\n")),
        ("worlds/box/data/seed.json", "{\"n\": 0}\n".into()),
        ("scenarios/poke.yaml", "format: shakedown-scenario/1\nlabel: Poke it\nworld: box\ntask: Poke the box once.\npreset: t1\n".into()),
        ("setups/solo.yaml", setup.into()),
        ("experiments/try.yaml", "format: shakedown-experiment/1\nlabel: Try it\nworld: box\nscenario: poke\nsetups: [solo]\nrepeats: {per_setup: 1}\n".into()),
        ("ops/compose.yaml", "services:\n  cache:\n    image: redis:6.2-alpine\n    volumes: [cache-data:/data]\nvolumes:\n  cache-data: {}\n".into()),
    ]
}

/// The fixture's packages: the Python part `<p>-part` (the toy program, run with uv) and the
/// benchmark `<p>-bench` over it.
pub fn package_files(p: &str) -> Vec<(&'static str, String)> {
    let manifest = |kind: &str, name: &str, extra: &str, argv: &str| {
        format!("kind = \"{kind}\"\nname = \"{name}\"\nversion = \"0.1.0\"\nprotocol = 1\n{extra}run = {{ command = [{argv}] }}\n")
    };
    let part = r#""uv", "run", "--project", ".", "python", "toy.py", "part""#;
    let bench = format!(
        r#""uv", "run", "--project", "../parts/toy", "python", "../parts/toy/toy.py", "bench", "--name", "{p}-bench", "--integration", "{p}-part", "--scored""#
    );
    vec![
        ("parts/toy/pyproject.toml", "[project]\nname = \"toy-part\"\nversion = \"0.1.0\"\nrequires-python = \">=3.8\"\ndependencies = []\n\n[tool.uv]\npackage = false\n".into()),
        ("parts/toy/shakedown-package.toml", manifest("integration", &format!("{p}-part"), "", part)),
        ("bench/shakedown-package.toml", manifest("benchmark", &format!("{p}-bench"), "opt_in = true\n", &bench)),
    ]
}

/// A repo holding the fixture project and its packages, and its first commit.
pub fn fixture_repo(p: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    for (file, text) in project_files(p).into_iter().chain(package_files(p)) {
        write(r, file, &text);
    }
    let toy = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/toy.py");
    std::fs::copy(toy, r.join("parts/toy/toy.py")).unwrap();
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "first"]);
    let commit = git(r, &["rev-parse", "HEAD"]);
    (dir, commit)
}

pub fn commit_change(repo: &Path, file: &str, text: &str) -> String {
    write(repo, file, text);
    git(repo, &["commit", "-q", "-am", "change"]);
    git(repo, &["rev-parse", "HEAD"])
}
