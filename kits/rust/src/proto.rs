//! Wire types of shakedown-part/1 (called arena-part/1 until 2026-11-19). See docs/protocol.md.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

mod scenario;
mod view;

pub use scenario::*;
pub use view::*;

pub const PROTOCOL: u32 = 1;
/// The protocol's name, as the kits announce it in `describe`'s `protocol`.
pub const PROTOCOL_NAME: &str = "shakedown-part/1";
/// Its name until 2026-11-19, still accepted.
pub const OLD_PROTOCOL_NAME: &str = "arena-part/1";

/// The protocol version a manifest's or a `describe`'s `protocol` says: the number, or either name
/// of version 1 (None: neither).
pub fn protocol_version(v: &Value) -> Option<u32> {
    match v {
        Value::Number(n) => n.as_u64().and_then(|n| u32::try_from(n).ok()),
        Value::String(s) if s == PROTOCOL_NAME || s == OLD_PROTOCOL_NAME => Some(PROTOCOL),
        _ => None,
    }
}

/// `protocol`: a number, or a name of the protocol ([`protocol_version`]).
fn de_protocol<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let v = Value::deserialize(d)?;
    protocol_version(&v).ok_or_else(|| serde::de::Error::custom(format!("unknown protocol {v}")))
}

fn all(s: &str, ok: impl Fn(u8) -> bool, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && s.bytes().all(ok)
}

/// Instance and save ids: `^[a-z0-9_]{1,63}$`.
pub fn valid_id(s: &str) -> bool {
    all(
        s,
        |b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_',
        63,
    )
}

/// A `gc` keep entry: a save id, or an id prefix followed by `*` (`base_*` keeps every save whose
/// id starts with `base_`).
pub fn valid_keep(s: &str) -> bool {
    match s.strip_suffix('*') {
        Some(prefix) => prefix.is_empty() || valid_id(prefix),
        None => valid_id(s),
    }
}

/// True when `keep` (`gc` entries, see `valid_keep`) keeps the save `id`.
pub fn kept<S: AsRef<str>>(keep: &[S], id: &str) -> bool {
    keep.iter().any(|k| match k.as_ref().strip_suffix('*') {
        Some(prefix) => id.starts_with(prefix),
        None => k.as_ref() == id,
    })
}

/// Package names: `^[a-z0-9-]{1,40}$`.
pub fn valid_package_name(s: &str) -> bool {
    all(
        s,
        |b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-',
        40,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExactKind {
    Save,
    Replay,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Restore {
    pub exact: ExactKind,
    pub approximate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effect {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    pub effect: Effect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Describe {
    pub name: String,
    pub version: String,
    #[serde(deserialize_with = "de_protocol")]
    pub protocol: u32,
    pub restore: Restore,
    pub uses_clock: bool,
    pub fingerprint: Value,
    pub tools: Vec<ToolSchema>,
    /// What kind of system this part is, for monitoring views (absent: not said).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemInfo>,
    /// When shaked saves the part (absent: after every step that wrote to it).
    #[serde(default, skip_serializing_if = "SaveAt::is_step")]
    pub save_at: SaveAt,
}

/// When shaked saves a part: after each step that wrote to it (`step`, the default), or only at
/// save points (`points`: before a model call when it changed, on request, at run end).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SaveAt {
    #[default]
    Step,
    Points,
}

impl SaveAt {
    pub fn is_step(&self) -> bool {
        *self == SaveAt::Step
    }
}

/// The world's time at `start`: the unix ms of clock 0 and how fast the clock runs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WorldTimeParams {
    pub epoch_ms: i64,
    pub speed: f64,
}

/// The header an upstream answers a proxied request with to say whether it read or wrote
/// (`read` or `write`); shaked strips it before the client sees the answer.
pub const EFFECT_HEADER: &str = "x-arena-effect";

/// The header shaked's proxy names the agent behind a request with (a client's own copy is
/// dropped); none for a driver's request no agent's model call came before.
pub const AGENT_HEADER: &str = "x-arena-agent";

/// The header (`1`) shaked's proxy marks a read-only agent's request with, sent only to routes
/// that say `honours_readonly`: the part must then refuse its writes itself.
pub const READONLY_HEADER: &str = "x-arena-readonly";

/// A host an instance serves through shaked's proxy: requests to `host` go to `upstream`. `env`
/// is what drivers and agents are given to find it (`GITHUB_API_URL`); each of `reads` is a
/// `METHOD glob` pattern of requests that only read (`POST */git-upload-pack`). With
/// `honours_readonly`, a read-only agent's requests are sent on marked `READONLY_HEADER` (the part
/// refuses their writes); without it shaked refuses those that write by the method rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HttpRoute {
    pub host: String,
    pub upstream: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reads: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub honours_readonly: bool,
}

/// `http_routes`: the hosts an instance serves.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HttpRoutes {
    pub routes: Vec<HttpRoute>,
}

/// The kind of system a part is: `kind` is one of "erp", "crm", "database", "api", "mcp",
/// "messaging", "approvals", "plant", "lims", "files", "logistics" or another short word;
/// `product` is what runs ("ERPNext"); `stands_in_for` names the commercial systems an
/// open-source stand-in represents ("SAP S/4HANA"); `simulated` is true when the part is a
/// simulation, not the real product.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemInfo {
    pub kind: String,
    pub product: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stands_in_for: Option<String>,
    #[serde(default)]
    pub simulated: bool,
}

impl SystemInfo {
    /// A real product: `kind`, `product`, and what it stands in for (if anything).
    pub fn real(kind: &str, product: &str, stands_in_for: Option<&str>) -> Self {
        SystemInfo {
            kind: kind.into(),
            product: product.into(),
            stands_in_for: stands_in_for.map(Into::into),
            simulated: false,
        }
    }

    /// A simulation of a kind of system.
    pub fn simulated(kind: &str, product: &str, stands_in_for: Option<&str>) -> Self {
        SystemInfo {
            simulated: true,
            ..SystemInfo::real(kind, product, stands_in_for)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Exact,
    Approximate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FromSave {
    pub save: String,
    pub mode: Mode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StartParams {
    pub instance: String,
    #[serde(default)]
    pub config: Value,
    pub seed: u64,
    pub clock_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<FromSave>,
    /// The world's time (absent: the part is not told it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_time: Option<WorldTimeParams>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallToolParams {
    pub instance: String,
    pub name: String,
    #[serde(default)]
    pub args: Value,
    pub clock_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub reads: Vec<String>,
    #[serde(default)]
    pub writes: Vec<String>,
    /// Events the call itself gives the world: recorded, and matched against the world's links.
    #[serde(default, skip_serializing_if = "<[_]>::is_empty")]
    pub events: Box<[WorldEvent]>,
}

/// True for a key that is the part's own bookkeeping, which no agent's call is about: its kind
/// starts with `_` (`_disruption/courier-capacity`). See docs/protocol.md, "Ids and keys".
pub fn is_internal_key(key: &str) -> bool {
    key.starts_with('_')
}

impl ToolOutput {
    /// `writes` split into the world's keys and the part's internal ones, in order.
    pub fn split_writes(writes: Vec<String>) -> (Vec<String>, Vec<String>) {
        writes.into_iter().partition(|k| !is_internal_key(k))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdvanceParams {
    pub instance: String,
    pub clock_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldEvent {
    pub kind: String,
    pub text: String,
    #[serde(default)]
    pub data: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Advanced {
    pub changed: bool,
    #[serde(default)]
    pub events: Vec<WorldEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveParams {
    pub instance: String,
    pub save: String,
    pub dir: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub bytes: u64,
    pub exact: bool,
}

/// What `inspect` and `diff` look at: a live instance or a save.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Target {
    Instance { instance: String },
    Save { save: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InspectParams {
    pub target: Target,
    #[serde(default)]
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Docs {
    pub docs: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffParams {
    pub a: Target,
    pub b: Target,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Differing {
    pub differing: Vec<String>,
}

impl Differing {
    /// The keys whose docs differ between `a` and `b` (a key only one side has differs), sorted.
    pub fn between<V: PartialEq>(a: &BTreeMap<String, V>, b: &BTreeMap<String, V>) -> Differing {
        let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
        let differing = keys
            .into_iter()
            .filter(|k| a.get(*k) != b.get(*k))
            .cloned()
            .collect();
        Differing { differing }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IdParams {
    pub instance: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveId {
    pub save: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GcParams {
    pub keep: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GcResult {
    pub dropped: u64,
    pub bytes: u64,
}

// ---- Benchmark side ----

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldPart {
    pub part: String,
    pub integration: String,
    #[serde(default)]
    pub config: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: String,
    pub reason: String,
    pub ticket: String,
    /// Calls shaked makes on a run's world right after it starts, before any agent: the task's
    /// scripted setup (a scenario, a disruption). The tools they call are the world's own: agents
    /// don't get them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub setup: Vec<SetupCall>,
    /// The world's date and time at clock 0 (`YYYY-MM-DDTHH:MM:SS`, the world's local time), for a
    /// world whose clock is not today's: shaked then tells every agent the world's current date and
    /// time (clock 0 plus the branch clock), so agents read the world's dates against its own clock
    /// rather than the date their environment reports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock_start: Option<String>,
}

/// One call of a task's setup: `tool` of the part `part`, with `args`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetupCall {
    pub part: String,
    pub tool: String,
    #[serde(default)]
    pub args: Value,
}

/// One agent of a benchmark's suggested team: its id (`^[a-z0-9_-]+$`), name and system prompt,
/// and whether the team keeps it from writing (absent: it may use every tool).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamRole {
    pub id: String,
    pub name: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub read_only: bool,
    /// The world tools the role may see and call (empty: every tool).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
}

/// A link between two parts: an event of `from` of kind `event` calls `to`'s `tool`. In `args`, a
/// string starting with `/` is a JSON pointer into the event's `data`; anything else is a constant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub from: String,
    pub event: String,
    pub to: String,
    pub tool: String,
    #[serde(default)]
    pub args: Map<String, Value>,
}

/// A gate on a tool: the call goes through only if a document of `part` says so. `key` is the
/// document's key, with `{arg}` standing for the call's argument `arg`. Each entry of `require` is
/// a JSON pointer into the document and the value it must have; the string `"{arg}"` is the call's
/// argument. A world's gates hold for agents' calls, not a link's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gate {
    pub tool: String,
    pub part: String,
    pub key: String,
    #[serde(default)]
    pub require: Map<String, Value>,
}

/// A team a benchmark suggests: its agents in the order they run, and the shaked runner its
/// prompts are written for (`chain`, `concurrent` or `dispatch`; absent: any). In a `dispatch`
/// run the first agent leads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchTeam {
    pub id: String,
    pub name: String,
    pub agents: Vec<TeamRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BenchDescribe {
    pub name: String,
    pub version: String,
    #[serde(deserialize_with = "de_protocol")]
    pub protocol: u32,
    pub world: Vec<WorldPart>,
    pub tasks: Vec<TaskInfo>,
    pub instructions: String,
    /// The heading over `instructions` in each agent's input; shaked uses a generic one if absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions_title: Option<String>,
    /// The team the benchmark suggests; shaked suggests a generic one if absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<BenchTeam>,
    /// Other teams it suggests beside `team` (a dispatching team, a chain), offered one click
    /// away.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub teams: Vec<BenchTeam>,
    pub tools: Vec<ToolSchema>,
    /// Calls between the parts of the world.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Link>,
    /// Conditions on tools, checked against other parts' documents.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gates: Vec<Gate>,
    /// True: a branch is scored by the benchmark's `score` over the world's end state, and no gold
    /// world is made.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub scored: bool,
    /// The disruptions a run's scenario builder may add.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario_options: Option<ScenarioOptions>,
    /// Present when the benchmark answers `view` (the world's digital twin).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<ViewInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeedParams {
    pub connections: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchCallTool {
    pub name: String,
    #[serde(default)]
    pub args: Value,
    pub connections: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BenchToolOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub part: String,
    #[serde(default)]
    pub reads: Vec<String>,
    #[serde(default)]
    pub writes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoldParams {
    pub task: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub name: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GoldActions {
    pub actions: Vec<Action>,
}

/// `score`: a task and every part's end state, `{part: {key: doc}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreParams {
    pub task: String,
    pub docs: Map<String, Value>,
}

/// One named pass-or-fail check of a branch's end state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedCheck {
    pub name: String,
    pub passed: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    /// A hard safety check (an unsafe action, not a missed goal): reported apart from the score.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub safety: bool,
    /// The figure the check measured, when it has one (an optimisation's verified objective, the
    /// simulations a study used): experiments show it per setup beside the pass rate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measure: Option<Measure>,
}

/// Which way a measure is better.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Better {
    Higher,
    Lower,
}

/// A number a named check measured, with what it means. `better` says which way is better (absent:
/// neither, as for a count); `target` is the value the task asks for; `reference` a value to read it
/// against that no run made (an offline baseline search).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measure {
    pub label: String,
    pub value: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub better: Option<Better>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<Reference>,
}

/// A labelled value a measure is read against.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reference {
    pub label: String,
    pub value: f64,
}

impl NamedCheck {
    /// A check whose `detail` is `fails_with` when it failed and empty when it passed.
    pub fn new(name: &str, passed: bool, fails_with: String, safety: bool) -> NamedCheck {
        NamedCheck {
            name: name.into(),
            passed,
            detail: if passed { String::new() } else { fails_with },
            safety,
            measure: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Score {
    pub checks: Vec<NamedCheck>,
}

#[cfg(test)]
mod protocol_names {
    use super::*;

    #[test]
    fn both_names_and_the_number_say_version_1() {
        for v in [
            json_n(1),
            Value::from(PROTOCOL_NAME),
            Value::from(OLD_PROTOCOL_NAME),
        ] {
            assert_eq!(protocol_version(&v), Some(PROTOCOL), "{v}");
        }
        assert_eq!(protocol_version(&Value::from("other-part/1")), None);
        assert_eq!(protocol_version(&json_n(2)), Some(2));
    }

    fn json_n(n: u64) -> Value {
        Value::from(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_check_may_carry_a_measure_and_one_without_reads_as_before() {
        let with: NamedCheck = serde_json::from_value(serde_json::json!({
            "name": "goal_reached", "passed": true,
            "measure": {"label": "Line 2 OEE", "value": 86.2, "unit": "%", "better": "higher", "target": 85,
                        "reference": {"label": "Random search", "value": 79.5}}}))
        .unwrap();
        let m = with.measure.as_ref().unwrap();
        assert_eq!(
            (m.better, m.target, m.reference.as_ref().unwrap().value),
            (Some(Better::Higher), Some(85.0), 79.5)
        );
        let plain = NamedCheck::new("a", true, String::new(), false);
        assert_eq!(
            serde_json::to_value(&plain).unwrap(),
            serde_json::json!({"name": "a", "passed": true})
        );
    }

    #[test]
    fn internal_keys_are_split_from_the_worlds_writes() {
        let writes = ["_disruption/a", "quote/Q-1", "_peeks/n"]
            .map(String::from)
            .to_vec();
        let (world, internal) = ToolOutput::split_writes(writes);
        assert_eq!(
            (world, internal),
            (
                vec!["quote/Q-1".to_string()],
                vec!["_disruption/a".to_string(), "_peeks/n".to_string()]
            )
        );
        assert!(!is_internal_key("disruption/a"));
    }

    #[test]
    fn ids() {
        assert!(valid_id("snap_1"));
        assert!(!valid_id("Bad Id"));
        assert!(!valid_id(""));
        assert!(!valid_id(&"a".repeat(64)));
        assert!(valid_package_name("ridgeline-plant"));
        assert!(!valid_package_name("a_b"));
    }

    #[test]
    fn keep_entries() {
        assert!(valid_keep("s_1") && valid_keep("base_*") && valid_keep("*"));
        assert!(!valid_keep("Base_*") && !valid_keep("a*b") && !valid_keep("**"));
        let keep = ["s_1", "gold_*"];
        assert!(kept(&keep, "s_1") && kept(&keep, "gold_x_1"));
        assert!(!kept(&keep, "s_12") && !kept(&keep, "xgold_1"));
    }

    #[test]
    fn differing_between_maps() {
        let a: BTreeMap<String, i32> = [("x".into(), 1), ("y".into(), 2)].into();
        let b: BTreeMap<String, i32> = [("y".into(), 3), ("z".into(), 4), ("x".into(), 1)].into();
        assert_eq!(Differing::between(&a, &b).differing, ["y", "z"]);
        assert!(Differing::between(&a, &a).differing.is_empty());
    }

    #[test]
    fn describe_without_save_at_reads_as_step_and_writes_none() {
        let d: Describe = serde_json::from_value(serde_json::json!({"name": "x", "version": "1",
            "protocol": 1, "restore": {"exact": "save", "approximate": false},
            "uses_clock": false, "fingerprint": {}, "tools": []}))
        .unwrap();
        assert_eq!(d.save_at, SaveAt::Step);
        assert!(serde_json::to_value(&d).unwrap().get("save_at").is_none());
        let p: SaveAt = serde_json::from_value(serde_json::json!("points")).unwrap();
        assert_eq!(p, SaveAt::Points);
        let d = Describe {
            save_at: SaveAt::Points,
            ..d
        };
        assert_eq!(serde_json::to_value(&d).unwrap()["save_at"], "points");
    }

    #[test]
    fn start_params_carry_world_time_only_when_given() {
        let mut s = StartParams {
            instance: "i".into(),
            config: serde_json::json!({}),
            seed: 1,
            clock_ms: 0,
            from: None,
            world_time: None,
        };
        assert!(serde_json::to_value(&s)
            .unwrap()
            .get("world_time")
            .is_none());
        s.world_time = Some(WorldTimeParams {
            epoch_ms: 1_773_122_400_000,
            speed: 3.0,
        });
        assert_eq!(
            serde_json::to_value(&s).unwrap()["world_time"],
            serde_json::json!({"epoch_ms": 1_773_122_400_000i64, "speed": 3.0})
        );
    }

    #[test]
    fn an_http_route_needs_only_host_and_upstream() {
        let r: HttpRoute = serde_json::from_value(
            serde_json::json!({"host": "a.test", "upstream": "http://127.0.0.1:9"}),
        )
        .unwrap();
        assert!(r.env.is_empty() && r.reads.is_empty() && !r.honours_readonly);
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"host": "a.test", "upstream": "http://127.0.0.1:9"})
        );
        let none: HttpRoutes = serde_json::from_value(serde_json::json!({"routes": []})).unwrap();
        assert_eq!(none, HttpRoutes::default());
    }

    #[test]
    fn target_untagged() {
        let t: Target = serde_json::from_str(r#"{"save":"s1"}"#).unwrap();
        assert_eq!(t, Target::Save { save: "s1".into() });
    }
}
