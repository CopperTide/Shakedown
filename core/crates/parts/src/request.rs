//! The protocol's requests, typed: each params type of `proto` and the method it is sent as
//! (docs/protocol.md). [`crate::Client::request`] sends one; the caller picks the answer's type.

use serde::Serialize;

use crate::proto::{
    AdvanceParams, BenchCallTool, CallToolParams, DiffParams, GcParams, GoldParams, IdParams,
    InspectParams, SaveId, SaveParams, ScoreParams, SeedParams, StartParams, ViewParams,
};

/// The params of one protocol method.
pub trait Request: Serialize {
    const METHOD: &'static str;
}

macro_rules! requests {
    ($($params:ty => $method:literal,)+) => {
        $(impl Request for $params {
            const METHOD: &'static str = $method;
        })+
    };
}

requests! {
    StartParams => "start",
    CallToolParams => "call_tool",
    AdvanceParams => "advance",
    SaveParams => "save",
    InspectParams => "inspect",
    DiffParams => "diff",
    SaveId => "drop_save",
    GcParams => "gc",
    Stop => "stop",
    Connect => "connect",
    GetHttpRoutes => "http_routes",
    Describe => "describe",
    GetUsage => "usage",
    SeedParams => "seed",
    BenchCallTool => "call_tool",
    GoldParams => "gold_actions",
    ScoreParams => "score",
    ViewParams => "view",
    ListTemplates => "templates",
    GetTemplate => "template",
    PreviewWorld => "preview",
    GenerateWorld => "generate",
}

/// A builder's `templates` (no params: `{}`).
#[derive(Debug, Clone, Serialize)]
pub struct ListTemplates {}

/// A builder's `template`: the form of template `id`.
#[derive(Debug, Clone, Serialize)]
pub struct GetTemplate {
    pub id: String,
}

/// A builder's `preview` of the world `choices` make of template `id`.
#[derive(Debug, Clone, Serialize)]
pub struct PreviewWorld {
    pub id: String,
    pub choices: serde_json::Value,
}

/// A builder's `generate`: the world written as a package into `dir`.
#[derive(Debug, Clone, Serialize)]
pub struct GenerateWorld {
    pub id: String,
    pub choices: serde_json::Value,
    pub dir: String,
}

/// `stop` an instance.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct Stop(pub IdParams);

/// `connect`: how a benchmark reaches an instance.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct Connect(pub IdParams);

/// `http_routes`: the hosts an instance serves through arenad's proxy.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct GetHttpRoutes(pub IdParams);

/// `describe` a program (no params: `{}`).
#[derive(Debug, Clone, Serialize)]
pub struct Describe {}

/// The bytes a part keeps outside its saves (no params: `{}`).
#[derive(Debug, Clone, Serialize)]
pub struct GetUsage {}

impl Stop {
    pub fn instance(instance: impl Into<String>) -> Stop {
        Stop(IdParams {
            instance: instance.into(),
        })
    }
}

impl Connect {
    pub fn instance(instance: impl Into<String>) -> Connect {
        Connect(IdParams {
            instance: instance.into(),
        })
    }
}

impl GetHttpRoutes {
    pub fn instance(instance: impl Into<String>) -> GetHttpRoutes {
        GetHttpRoutes(IdParams {
            instance: instance.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::proto::Target;

    fn sent<R: Request>(r: &R) -> (&'static str, Value) {
        (R::METHOD, serde_json::to_value(r).unwrap())
    }

    #[test]
    fn the_requests_of_no_proto_type_are_sent_as_the_protocol_names_them() {
        let stop = sent(&Stop::instance("i"));
        assert_eq!(stop, ("stop", json!({"instance": "i"})));
        let connect = sent(&Connect::instance("i"));
        assert_eq!(connect, ("connect", json!({"instance": "i"})));
        assert_eq!(sent(&Describe {}), ("describe", json!({})));
        assert_eq!(sent(&GetUsage {}), ("usage", json!({})));
        let routes = sent(&GetHttpRoutes::instance("i"));
        assert_eq!(routes, ("http_routes", json!({"instance": "i"})));
    }

    #[test]
    fn proto_params_are_sent_with_their_method() {
        let inspect = InspectParams {
            target: Target::Save { save: "s".into() },
            keys: vec![],
        };
        let want = json!({"target": {"save": "s"}, "keys": []});
        assert_eq!(sent(&inspect), ("inspect", want));
        let start = StartParams {
            instance: "i".into(),
            config: json!({"k": 1}),
            seed: 1,
            clock_ms: 0,
            from: None,
            world_time: None,
        };
        let want = json!({"instance": "i", "config": {"k": 1}, "seed": 1, "clock_ms": 0});
        assert_eq!(sent(&start), ("start", want));
        let call = CallToolParams {
            instance: "i".into(),
            name: "t".into(),
            args: json!({}),
            clock_ms: 5,
        };
        let want = json!({"instance": "i", "name": "t", "args": {}, "clock_ms": 5});
        assert_eq!(sent(&call), ("call_tool", want));
    }

    #[test]
    fn builder_requests_are_sent_as_the_builder_calls() {
        assert_eq!(sent(&ListTemplates {}), ("templates", json!({})));
        let one = GetTemplate { id: "shop".into() };
        assert_eq!(sent(&one), ("template", json!({"id": "shop"})));
        let preview = PreviewWorld {
            id: "shop".into(),
            choices: json!({}),
        };
        let want = json!({"id": "shop", "choices": {}});
        assert_eq!(sent(&preview), ("preview", want));
        let generate = GenerateWorld {
            id: "shop".into(),
            choices: json!({"size": 3}),
            dir: "/w/acme".into(),
        };
        let want = json!({"id": "shop", "choices": {"size": 3}, "dir": "/w/acme"});
        assert_eq!(sent(&generate), ("generate", want));
    }
}
