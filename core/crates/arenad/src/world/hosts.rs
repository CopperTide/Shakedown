//! The hosts a world's parts serve through arenad's proxy (`http_routes`), and which part serves
//! each: requests to a host go to that part's upstream and are steps of that part.

use std::collections::BTreeMap;

use parts::proto::{ExactKind, HttpRoute, Restore};

/// The tool name of a request through the proxy: no part's tool may have it.
pub const HTTP_TOOL: &str = "http";

/// A host one part serves: where its requests go and which of them only read.
#[derive(Debug, Clone)]
pub struct HostRoute {
    pub part: String,
    pub host: String,
    pub upstream: reqwest::Url,
    pub env: BTreeMap<String, String>,
    /// `(METHOD, path glob)` of requests that only read.
    reads: Vec<(String, String)>,
    /// True when the part refuses a read-only agent's writes itself (see `HttpRoute`).
    pub honours_readonly: bool,
}

impl HostRoute {
    /// True when a `method` request to `path` only reads: GET, HEAD and OPTIONS, and the
    /// requests the route's patterns list.
    pub fn is_read(&self, method: &str, path: &str) -> bool {
        matches!(method, "GET" | "HEAD" | "OPTIONS")
            || (self.reads.iter()).any(|(m, glob)| m == method && glob_matches(glob, path))
    }

    /// The upstream URL of a request for `path_and_query` (`/repos/a?page=2`).
    pub fn target(&self, path_and_query: &str) -> reqwest::Url {
        let mut url = self.upstream.clone();
        let (path, query) = match path_and_query.split_once('?') {
            Some((p, q)) => (p, Some(q)),
            None => (path_and_query, None),
        };
        let base = self.upstream.path().trim_end_matches('/');
        url.set_path(&format!("{base}{path}"));
        url.set_query(query);
        url
    }
}

/// True when `text` matches `glob`, whose `*` matches any run of characters (`/` included).
fn glob_matches(glob: &str, text: &str) -> bool {
    match glob.split_once('*') {
        None => glob == text,
        Some((head, rest)) => {
            let Some(text) = text.strip_prefix(head) else {
                return false;
            };
            (0..=text.len())
                .filter(|&i| text.is_char_boundary(i))
                .any(|i| glob_matches(rest, &text[i..]))
        }
    }
}

/// Every host a world's parts serve.
#[derive(Debug, Default)]
pub struct Hosts {
    routes: Vec<HostRoute>,
}

impl Hosts {
    /// The hosts of `served` (each part's name, restore and routes). Refuses a bad route, a part
    /// that serves HTTP but restores by replay, and two parts serving one host or setting one
    /// variable.
    pub fn build(served: Vec<(String, Restore, Vec<HttpRoute>)>) -> Result<Hosts, String> {
        let mut hosts = Hosts::default();
        for (part, restore, routes) in served {
            if !routes.is_empty() && restore.exact == ExactKind::Replay {
                return Err(format!(
                    "the part {part} serves HTTP but restores by replay; a part that serves HTTP \
                     must restore from saves"
                ));
            }
            for r in routes {
                let route = host_route(&part, r)
                    .map_err(|why| format!("the part {part} gives a bad route: {why}"))?;
                hosts.add(route)?;
            }
        }
        Ok(hosts)
    }

    /// The hosts with `part`'s routes replaced by `routes` (checked as `build` checks them); the
    /// hosts stay as they were when one is refused.
    pub fn replace(&mut self, part: &str, routes: Vec<HttpRoute>) -> Result<(), String> {
        let others = self.routes.iter().filter(|r| r.part != part).cloned();
        let mut next = Hosts {
            routes: others.collect(),
        };
        for r in routes {
            let route = host_route(part, r)
                .map_err(|why| format!("the part {part} gives a bad route: {why}"))?;
            next.add(route)?;
        }
        *self = next;
        Ok(())
    }

    /// Adds `route` unless another part serves its host or sets one of its variables.
    fn add(&mut self, route: HostRoute) -> Result<(), String> {
        for other in &self.routes {
            if other.host == route.host {
                let (a, b, h) = (&other.part, &route.part, &route.host);
                return Err(format!("the parts {a} and {b} both serve the host {h}"));
            }
            let shared = route.env.keys().find(|k| other.env.contains_key(*k));
            if let Some(var) = shared.filter(|_| other.part != route.part) {
                let (a, b) = (&other.part, &route.part);
                return Err(format!("the parts {a} and {b} both set {var}"));
            }
        }
        self.routes.push(route);
        Ok(())
    }

    /// The route of the host in `authority` (`Host` or `CONNECT` target), whatever its case and
    /// port.
    pub fn route(&self, authority: &str) -> Option<&HostRoute> {
        let host = strip_port(authority).to_ascii_lowercase();
        self.routes.iter().find(|r| r.host == host)
    }

    /// The variables every route gives drivers and agents.
    pub fn env(&self) -> BTreeMap<String, String> {
        let pairs = self.routes.iter().flat_map(|r| r.env.clone());
        pairs.collect()
    }

    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }
}

/// `authority` without its `:port`.
fn strip_port(authority: &str) -> &str {
    match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => host,
        _ => authority,
    }
}

/// `r` checked: a host of `[a-z0-9.-]`, an http(s) upstream, variable names of
/// `[A-Z_][A-Z0-9_]*` and `METHOD glob` patterns.
fn host_route(part: &str, r: HttpRoute) -> Result<HostRoute, String> {
    let host_ok = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-';
    if r.host.is_empty() || !r.host.bytes().all(host_ok) {
        return Err(format!("the host {:?} is not of [a-z0-9.-]", r.host));
    }
    let upstream = reqwest::Url::parse(&r.upstream)
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https") && u.has_host())
        .ok_or_else(|| format!("the upstream {:?} is no http(s) URL", r.upstream))?;
    if let Some(bad) = r.env.keys().find(|k| !is_env_name(k)) {
        return Err(format!(
            "the variable name {bad:?} is not of [A-Z_][A-Z0-9_]*"
        ));
    }
    let reads = (r.reads.iter())
        .map(|p| read_pattern(p))
        .collect::<Result<_, _>>()?;
    Ok(HostRoute {
        part: part.to_string(),
        host: r.host,
        upstream,
        env: r.env,
        reads,
        honours_readonly: r.honours_readonly,
    })
}

fn is_env_name(k: &str) -> bool {
    let mut bytes = k.bytes();
    let first = bytes
        .next()
        .is_some_and(|b| b.is_ascii_uppercase() || b == b'_');
    first && bytes.all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

/// A `METHOD glob` pattern, split.
fn read_pattern(p: &str) -> Result<(String, String), String> {
    let bad = || format!("the read pattern {p:?} is not METHOD glob");
    let (method, glob) = p.split_once(' ').ok_or_else(bad)?;
    let method_ok = !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase());
    if !method_ok || glob.is_empty() || glob.contains(' ') {
        return Err(bad());
    }
    Ok((method.to_string(), glob.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAVES: Restore = Restore {
        exact: ExactKind::Save,
        approximate: false,
    };

    fn route(host: &str, upstream: &str) -> HttpRoute {
        HttpRoute {
            host: host.into(),
            upstream: upstream.into(),
            env: BTreeMap::new(),
            reads: vec![],
            honours_readonly: false,
        }
    }

    /// `(part, host)` pairs, each on its own upstream.
    fn build(served: &[(&str, &str)]) -> Result<Hosts, String> {
        let served = served.iter().map(|(p, h)| {
            let r = route(h, "http://127.0.0.1:9");
            (p.to_string(), SAVES, vec![r])
        });
        Hosts::build(served.collect())
    }

    fn hosts(served: &[(&str, &str, &str)]) -> Hosts {
        let served = served
            .iter()
            .map(|(p, h, u)| (p.to_string(), SAVES, vec![route(h, u)]));
        Hosts::build(served.collect()).unwrap()
    }

    fn route_with_reads(reads: &[&str]) -> HostRoute {
        let mut r = route("x.test", "http://127.0.0.1:9");
        r.reads = reads.iter().map(|s| s.to_string()).collect();
        let h = Hosts::build(vec![("box".into(), SAVES, vec![r])]).unwrap();
        h.route("x.test").unwrap().clone()
    }

    fn with_env(part: &str, host: &str, var: &str) -> (String, Restore, Vec<HttpRoute>) {
        let mut r = route(host, "http://127.0.0.1:9");
        r.env.insert(var.into(), format!("https://{host}"));
        (part.into(), SAVES, vec![r])
    }

    #[test]
    fn a_host_is_found_whatever_its_case_or_port() {
        let h = hosts(&[("box", "toy.test", "http://127.0.0.1:9")]);
        assert_eq!(h.route("TOY.test:443").unwrap().part, "box");
        assert_eq!(h.route("toy.test").unwrap().part, "box");
        assert!(h.route("other.test").is_none());
        assert!(!h.is_empty() && Hosts::default().is_empty());
    }

    #[test]
    fn two_parts_serving_one_host_are_refused() {
        let e = build(&[("a", "x.test"), ("b", "x.test")]).unwrap_err();
        assert_eq!(e, "the parts a and b both serve the host x.test");
    }

    #[test]
    fn two_parts_setting_one_variable_are_refused() {
        let served = vec![
            with_env("a", "a.test", "APP_URL"),
            with_env("b", "b.test", "APP_URL"),
        ];
        let e = Hosts::build(served).unwrap_err();
        assert_eq!(e, "the parts a and b both set APP_URL");
    }

    #[test]
    fn the_env_is_every_routes_variables() {
        let served = vec![
            with_env("a", "a.test", "A_URL"),
            with_env("b", "b.test", "B_URL"),
        ];
        let env = Hosts::build(served).unwrap().env();
        let want = [("A_URL", "https://a.test"), ("B_URL", "https://b.test")];
        let want: BTreeMap<String, String> =
            want.map(|(k, v)| (k.to_string(), v.to_string())).into();
        assert_eq!(env, want);
    }

    #[test]
    fn a_replayed_part_may_not_serve_http() {
        let replay = Restore {
            exact: ExactKind::Replay,
            approximate: false,
        };
        let served = vec![(
            "box".into(),
            replay,
            vec![route("x.test", "http://127.0.0.1:9")],
        )];
        let e = Hosts::build(served).unwrap_err();
        let want = "the part box serves HTTP but restores by replay; a part that serves HTTP \
                    must restore from saves";
        assert_eq!(e, want);
        let served_none = vec![("box".into(), replay, vec![])];
        assert!(Hosts::build(served_none).unwrap().is_empty());
    }

    #[test]
    fn bad_routes_are_refused() {
        let bad = |r: HttpRoute| Hosts::build(vec![("box".into(), SAVES, vec![r])]).unwrap_err();
        let e = bad(route("Bad Host", "http://127.0.0.1:9"));
        assert!(e.starts_with("the part box gives a bad route: "), "{e}");
        assert!(bad(route("x.test", "ftp://127.0.0.1:9")).contains("bad route"));
        assert!(bad(route("x.test", "nonsense")).contains("bad route"));
        let mut r = route("x.test", "http://127.0.0.1:9");
        r.env.insert("lower".into(), "v".into());
        assert!(bad(r).contains("bad route"));
        let mut r = route("x.test", "http://127.0.0.1:9");
        r.reads = vec!["*/git-upload-pack".into()];
        assert!(bad(r).contains("bad route"));
    }

    #[test]
    fn reads_are_gets_and_the_patterns_a_route_lists() {
        let r = route_with_reads(&["POST */git-upload-pack"]);
        assert!(r.is_read("GET", "/x") && r.is_read("POST", "/acme/app.git/git-upload-pack"));
        assert!(r.is_read("HEAD", "/x") && r.is_read("OPTIONS", "/x"));
        assert!(!r.is_read("POST", "/acme/app.git/git-receive-pack"));
        assert!(!r.is_read("PUT", "/a/git-upload-pack"));
        assert!(!r.is_read("POST", "/a/git-upload-pack/more"));
    }

    #[test]
    fn the_target_keeps_path_and_query() {
        let h = hosts(&[("box", "toy.test", "http://127.0.0.1:9")]);
        let url = h.route("toy.test").unwrap().target("/repos/a?page=2");
        assert_eq!(url.as_str(), "http://127.0.0.1:9/repos/a?page=2");
    }
}
