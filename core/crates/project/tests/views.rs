//! View files (`shakedown-view/1`): the airline's example parses and round-trips, a world lists
//! its views, and a view that names what it doesn't have is refused with a pointer to it.

use std::path::Path;

use shakedown_project::format::{parse, to_yaml, Layer, Panel, ViewFile, WorldFile};
use shakedown_project::{world_views, Id, Project};

fn example() -> String {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/examples/airline-occ-view.yaml");
    std::fs::read_to_string(path).expect("the example view exists")
}

#[test]
fn the_example_view_parses_and_round_trips() {
    let view: ViewFile = parse("occ.yaml", &example()).expect("parses");
    let again: ViewFile = parse("occ.yaml", &to_yaml(&view).unwrap()).expect("re-parses");
    assert_eq!(view, again);
    assert_eq!(view.screens[0].id, "overview");
    let Some(Panel::Map(map)) = view.panels.get("network") else {
        panic!("the overview has a map");
    };
    assert!(matches!(map.layers[1], Layer::Moving(_)));
}

fn refused(text: &str) -> (String, String) {
    let err = parse::<ViewFile>("v.yaml", text).unwrap_err();
    (err.pointer, err.message)
}

const HEAD: &str = "format: shakedown-view/1\nlabel: V\n";

#[test]
fn a_view_that_names_what_it_lacks_is_refused() {
    let feed = "panels:\n  f: {kind: feed, kinds: [alert], style: nope}\n";
    let (pointer, message) = refused(&format!(
        "{HEAD}{feed}screens: [{{id: s, label: S, areas: [f]}}]\n"
    ));
    assert_eq!(
        (pointer.as_str(), message.as_str()),
        ("/panels/f/style", "there is no style nope")
    );
    let feed = "panels:\n  f: {kind: feed, kinds: [alert]}\n";
    let (pointer, message) = refused(&format!(
        "{HEAD}{feed}screens: [{{id: s, label: S, areas: [\"f g\"]}}]\n"
    ));
    assert_eq!(
        (pointer.as_str(), message.as_str()),
        ("/screens/0/areas", "there is no panel g")
    );
    let map = "panels:\n  m:\n    kind: map\n    layers:\n      - {id: a, kind: arcs, data: {find: \"x/*\"}, origin: {layer: p, field: o}, dest: {layer: p, field: d}}\n";
    let (pointer, _) = refused(&format!(
        "{HEAD}{map}screens: [{{id: s, label: S, areas: [m]}}]\n"
    ));
    assert_eq!(pointer, "/panels/m/layers/0/origin/layer");
    let (pointer, _) = refused(&format!("{HEAD}panels: {{}}\nscreens: []\n"));
    assert_eq!(pointer, "/screens");
}

#[test]
fn a_world_lists_its_views_and_a_broken_one_is_a_problem() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("worlds/w");
    std::fs::create_dir_all(world.join("views")).unwrap();
    std::fs::write(
        root.path().join("shakedown.yaml"),
        "format: shakedown-project/1\nname: P\n",
    )
    .unwrap();
    let file = "format: shakedown-world/3\nlabel: W\nviews: [views/occ.yaml, views/bad.yaml]\n";
    std::fs::write(world.join("world.yaml"), file).unwrap();
    std::fs::write(world.join("views/occ.yaml"), example()).unwrap();
    std::fs::write(world.join("views/bad.yaml"), "format: shakedown-view/1\n").unwrap();
    let parsed: WorldFile = parse("world.yaml", file).unwrap();
    let views = world_views(root.path(), &Id::parse("w").unwrap(), &parsed);
    let ids: Vec<(&str, bool)> = views
        .iter()
        .map(|v| (v.id.as_str(), v.view.is_ok()))
        .collect();
    assert_eq!(ids, [("occ", true), ("bad", false)]);
    let project = Project::open(root.path()).unwrap();
    let problems: Vec<&str> = project
        .problems
        .iter()
        .map(|p| p.pointer.as_str())
        .collect();
    assert_eq!(problems, ["/views/1"]);
}

#[test]
fn a_view_path_outside_the_world_is_refused() {
    let err = parse::<WorldFile>(
        "w.yaml",
        "format: shakedown-world/3\nlabel: W\nviews: [../x.yaml]\n",
    )
    .unwrap_err();
    assert_eq!(err.pointer, "/views/0");
}
