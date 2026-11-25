//! `worlds/<id>/views/<view>.yaml`: a world's maps and dashboards, declared. Panels bind to the
//! parts' docs with the expected-state grammar's finders, styles map field values to tones, and
//! screens lay panels out on a grid of named areas. One renderer draws every view.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{Format, Invalid};

/// A view: its panels, the styles they use and the screens that lay them out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViewFile {
    /// Always `shakedown-view/1`.
    pub format: String,
    /// The view's display name.
    pub label: String,
    /// The part a binding reads when it names none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    /// Named styles: a row's tone is the first rule whose `where` holds.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub styles: BTreeMap<String, Vec<StyleRule>>,
    /// What is drawn, by id.
    pub panels: BTreeMap<String, Panel>,
    /// How panels are laid out; the first screen is the default.
    pub screens: Vec<Screen>,
}

/// One rule of a style.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StyleRule {
    /// Conditions on the row (`{field: value}` or `{field: {op: value}}`); none always holds.
    #[serde(default, rename = "where", skip_serializing_if = "Option::is_none")]
    pub when: Option<Map<String, Value>>,
    /// The row's tone when the rule holds.
    pub tone: Tone,
    /// A chip label shown in place of the value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// The renderer's fixed tones; a view never names a colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    #[default]
    Neutral,
    Active,
    Done,
    Warn,
    Critical,
    Accent,
}

/// A screen: a CSS grid whose named areas are panel ids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Screen {
    /// Unique within the view.
    pub id: String,
    /// Display name.
    pub label: String,
    /// `grid-template-areas` rows, e.g. `"kpis kpis"`; `.` is an empty cell.
    pub areas: Vec<String>,
    /// `grid-template-columns`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<String>,
    /// `grid-template-rows`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<String>,
}

/// A dotted path into a doc, or a list of them: the first present and not null.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Field {
    One(String),
    First(Vec<String>),
}

impl Field {
    /// The paths tried, in order.
    pub fn paths(&self) -> Vec<&str> {
        match self {
            Field::One(p) => vec![p.as_str()],
            Field::First(ps) => ps.iter().map(String::as_str).collect(),
        }
    }

    /// The name a column of this field goes by: its first path.
    pub fn name(&self) -> &str {
        self.paths().first().copied().unwrap_or("")
    }
}

/// Many docs: the keys of `part` matching `find` (`*` any run, `?` one character) whose fields
/// meet `where`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Find {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    pub find: String,
    #[serde(default, rename = "where", skip_serializing_if = "Option::is_none")]
    pub when: Option<Map<String, Value>>,
}

/// One doc's field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<Field>,
}

/// How many docs a find matches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Count {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    pub find: String,
    #[serde(default, rename = "where", skip_serializing_if = "Option::is_none")]
    pub when: Option<Map<String, Value>>,
    /// Always true: the value is the number of matches.
    pub count: bool,
}

/// A figure's value: one doc's field, or a count of docs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Figure {
    Key(KeyRef),
    Count(Count),
}

/// How a value is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Shown {
    #[default]
    Text,
    Int,
    Number,
    /// A value already in percent.
    Percent,
    /// A ratio shown as a percent (`0.93` is `93%`).
    RatioPct,
    Usd,
    /// Minutes as `h:mm`.
    Minutes,
    /// An ISO time or epoch ms, shown in UTC.
    Time,
    /// The row's tone and chip label.
    Chip,
}

/// A panel; every kind may have a title.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Panel {
    Kpis(KpisPanel),
    Map(MapPanel),
    Board(BoardPanel),
    Timeline(TimelinePanel),
    Bars(BarsPanel),
    Feed(FeedPanel),
    Chat(ChatPanel),
}

/// A strip of figures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KpisPanel {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub items: Vec<KpiItem>,
}

/// One figure. Its style's rules see `{value}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KpiItem {
    pub label: String,
    pub value: Figure,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<Shown>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
}

/// A map on free tiles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MapPanel {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// `[lon, lat]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub center: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom: Option<f64>,
    pub layers: Vec<Layer>,
}

/// A map layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Layer {
    Points(PointsLayer),
    Arcs(PathLayer),
    Moving(PathLayer),
}

impl Layer {
    /// The layer's id.
    pub fn id(&self) -> &str {
        match self {
            Layer::Points(p) => &p.id,
            Layer::Arcs(p) | Layer::Moving(p) => &p.id,
        }
    }
}

/// Docs drawn where their `lat` and `lon` say; `key` is what paths join on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PointsLayer {
    pub id: String,
    pub data: Find,
    pub lat: Field,
    pub lon: Field,
    pub key: Field,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
}

/// Docs drawn from one point to another: as arcs, or moving by the world clock from `depart` to
/// `arrive`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PathLayer {
    pub id: String,
    pub data: Find,
    pub origin: Join,
    pub dest: Join,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depart: Option<Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrive: Option<Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
}

/// The doc's `field` holds a `key` of the points layer `layer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Join {
    pub layer: String,
    pub field: Field,
}

/// A table of docs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardPanel {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub data: Find,
    pub columns: Vec<Column>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<Sort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Filter>,
    /// The most rows sent (all by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<usize>,
}

/// A board column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Column {
    pub field: Field,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<Shown>,
}

/// A sort order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Sort {
    pub field: Field,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub desc: bool,
}

/// A quick filter: the rows its `where` holds for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    pub id: String,
    pub label: String,
    #[serde(rename = "where")]
    pub when: Map<String, Value>,
}

/// Lanes of bars.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelinePanel {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub data: Find,
    pub lane: Field,
    pub start: Field,
    pub end: Field,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// What a lane is called, e.g. `Tail`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane_label: Option<String>,
}

/// A value against its limit, worst ratio first. Style rules also see `value`, `limit`, `ratio`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BarsPanel {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub data: Find,
    pub label: Field,
    pub value: BarValue,
    pub limit: Field,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// The most rows sent (40 by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<usize>,
}

/// A bar's value: a field, or the minutes since a time field by the world clock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum BarValue {
    Since(Since),
    Field(Field),
}

/// The minutes from the time in `since` to the world clock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Since {
    pub since: Field,
}

/// The world's events, newest first. Style rules see `{part, kind, text, data}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FeedPanel {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub kinds: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// The most events sent (60 by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<usize>,
}

/// A conversation: one message per doc, oldest first, as bubbles. Style rules see the doc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChatPanel {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub data: Find,
    /// Who said it.
    pub from: Field,
    /// What was said.
    pub text: Field,
    /// The order (by key by default, digit runs as numbers: `msg/2` before `msg/10`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<Sort>,
    /// The values of `from` drawn on the right (the others on the left).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub right: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// The last messages sent (all by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<usize>,
}

impl Panel {
    /// The styles the panel names, with where each is named.
    fn styles(&self) -> Vec<(String, String)> {
        let named = |at: &str, s: &Option<String>| s.clone().map(|s| (at.to_string(), s));
        match self {
            Panel::Kpis(k) => (k.items.iter().enumerate())
                .filter_map(|(i, item)| named(&format!("/items/{i}/style"), &item.style))
                .collect(),
            Panel::Map(m) => (m.layers.iter().enumerate())
                .filter_map(|(i, l)| {
                    let style = match l {
                        Layer::Points(p) => &p.style,
                        Layer::Arcs(p) | Layer::Moving(p) => &p.style,
                    };
                    named(&format!("/layers/{i}/style"), style)
                })
                .collect(),
            Panel::Board(b) => named("/style", &b.style).into_iter().collect(),
            Panel::Timeline(t) => named("/style", &t.style).into_iter().collect(),
            Panel::Bars(b) => named("/style", &b.style).into_iter().collect(),
            Panel::Feed(f) => named("/style", &f.style).into_iter().collect(),
            Panel::Chat(c) => named("/style", &c.style).into_iter().collect(),
        }
    }
}

impl Format for ViewFile {
    const FORMAT: &'static str = "shakedown-view/1";

    fn check(&self) -> Result<(), Invalid> {
        if self.screens.is_empty() {
            return Err(Invalid::new("/screens", "a view needs at least one screen"));
        }
        for (i, screen) in self.screens.iter().enumerate() {
            self.check_screen(i, screen)?;
        }
        for (id, panel) in &self.panels {
            self.check_panel(id, panel)?;
        }
        Ok(())
    }
}

impl ViewFile {
    fn check_screen(&self, i: usize, screen: &Screen) -> Result<(), Invalid> {
        let at = format!("/screens/{i}/areas");
        if screen.areas.is_empty() {
            return Err(Invalid::new(at, "a screen needs at least one row of areas"));
        }
        let names: BTreeSet<&str> = screen
            .areas
            .iter()
            .flat_map(|r| r.split_whitespace())
            .collect();
        match names
            .iter()
            .find(|n| **n != "." && !self.panels.contains_key(**n))
        {
            Some(n) => Err(Invalid::new(at, format!("there is no panel {n}"))),
            None => Ok(()),
        }
    }

    fn check_panel(&self, id: &str, panel: &Panel) -> Result<(), Invalid> {
        let at = |rest: &str| format!("/panels/{}{rest}", crate::secrets::escape(id));
        for (pointer, style) in panel.styles() {
            if !self.styles.contains_key(&style) {
                return Err(Invalid::new(
                    at(&pointer),
                    format!("there is no style {style}"),
                ));
            }
        }
        match panel {
            Panel::Map(m) => check_layers(&m.layers).map_err(|(p, m)| Invalid::new(at(&p), m)),
            _ => Ok(()),
        }
    }
}

/// Paths join on points layers of the same map, and a moving layer has its times.
fn check_layers(layers: &[Layer]) -> Result<(), (String, String)> {
    let points: BTreeSet<&str> = (layers.iter())
        .filter_map(|l| match l {
            Layer::Points(p) => Some(p.id.as_str()),
            _ => None,
        })
        .collect();
    for (i, layer) in layers.iter().enumerate() {
        let (path, moving) = match layer {
            Layer::Points(_) => continue,
            Layer::Arcs(p) => (p, false),
            Layer::Moving(p) => (p, true),
        };
        for (end, join) in [("origin", &path.origin), ("dest", &path.dest)] {
            if !points.contains(join.layer.as_str()) {
                let why = format!("there is no points layer {} in this map", join.layer);
                return Err((format!("/layers/{i}/{end}/layer"), why));
            }
        }
        if moving && (path.depart.is_none() || path.arrive.is_none()) {
            let why = "a moving layer needs depart and arrive".to_string();
            return Err((format!("/layers/{i}"), why));
        }
    }
    Ok(())
}
