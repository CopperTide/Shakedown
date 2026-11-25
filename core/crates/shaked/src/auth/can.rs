//! The engine's own checks, run before any layer's: a token's scope caps what it may do, and a run
//! token reaches only the run-internal actions of its own branch. Who may see what beyond that is
//! the layer's `authorize`.

use super::caller::{Caller, CallerKind, RunTokenKind, TokenScope};

/// What a caller asks to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    // On records.
    Read,
    Create,
    Start,
    Fork,
    Stop,
    Record,
    UseInRun,
    Rename,
    ChangeSetup,
    ReplaceSecret,
    DeleteFork,
    Delete,
    // On this shaked.
    ManageTokens,
    InstanceAdmin,
    // Inside a run (run tokens only).
    RegisterAgent,
    Report,
    Finish,
    CallTools,
    CallModel,
    WorkerLink,
}

/// The kinds of record a route acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    Run,
    Experiment,
    Suite,
    World,
    ProviderKey,
    Connection,
    Review,
    ApiToken,
    /// A world, scenario, setup or experiment file in the project folder.
    ProjectFile,
}

/// What an action is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'a> {
    Resource {
        kind: ResourceKind,
        id: &'a str,
    },
    Branch {
        run: &'a str,
        branch: &'a str,
    },
    /// Listing or creating records of a kind.
    Kind(ResourceKind),
    /// This shaked: settings, gc, tokens.
    Instance,
}

/// The answer: allowed, not found (the caller may not know it exists), or refused with a reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    NotFound,
    Forbidden(String),
}

/// The engine's checks for `c` doing `a` on `t`.
pub fn engine_check(c: &Caller, a: &Action, t: &Target<'_>) -> Decision {
    match &c.kind {
        CallerKind::Person => Decision::Allow,
        CallerKind::ApiToken { scopes, .. } => by_scope(scopes, a, t),
        CallerKind::RunToken {
            branch, kind: k, ..
        } => in_run(branch, *k, a, t),
    }
}

/// `read` views, `run` also starts, forks, stops and records (and drives a run it started:
/// registers agents, reports, finishes), `admin` does everything; no API token calls tools or
/// models or links a sandbox.
fn by_scope(scopes: &[TokenScope], a: &Action, t: &Target<'_>) -> Decision {
    use Action::*;
    let admin_only = |t: &Target<'_>| match t {
        Target::Kind(k) | Target::Resource { kind: k, .. } => {
            matches!(k, ResourceKind::ProviderKey | ResourceKind::ProjectFile)
        }
        _ => false,
    };
    let needs = match a {
        // Model keys steer every model call, and project files are what every run is built
        // from: only admin tokens change them.
        Create | ChangeSetup if admin_only(t) => TokenScope::Admin,
        Read => TokenScope::Read,
        Create | Start | Fork | Stop | Record | UseInRun | Rename | ChangeSetup | DeleteFork
        | RegisterAgent | Report | Finish => TokenScope::Run,
        ReplaceSecret | Delete | ManageTokens | InstanceAdmin => TokenScope::Admin,
        CallTools | CallModel | WorkerLink => {
            return forbidden("an API token does not call tools or models")
        }
    };
    let rank = |s: &TokenScope| match s {
        TokenScope::Read => 0,
        TokenScope::Run => 1,
        TokenScope::Admin => 2,
    };
    match scopes.iter().map(rank).max() {
        Some(top) if top >= rank(&needs) => Decision::Allow,
        _ => forbidden(&format!(
            "this token's scope does not cover that (needs {})",
            needs.as_str()
        )),
    }
}

/// A run token: its kind's actions on its own branch; another branch is not found.
fn in_run(own: &str, kind: RunTokenKind, a: &Action, t: &Target<'_>) -> Decision {
    use Action::*;
    let fits = match kind {
        RunTokenKind::Run => matches!(a, RegisterAgent | Report | Finish),
        // A sandbox's worker appends its branch's events and ends it over the link, and calls
        // the model gateway for its branch's agents.
        RunTokenKind::Worker => matches!(a, WorkerLink | CallModel),
        // Never a caller: an agent token is checked by its branch's roster, a ticket in memory.
        RunTokenKind::Agent | RunTokenKind::Ticket => false,
    };
    match t {
        Target::Branch { branch, .. } if fits && *branch == own => Decision::Allow,
        Target::Branch { .. } if fits => Decision::NotFound,
        _ => forbidden("a run token acts only inside its own branch"),
    }
}

fn forbidden(why: &str) -> Decision {
    Decision::Forbidden(why.to_string())
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn kind() -> Target<'static> {
        Target::Kind(ResourceKind::Run)
    }

    #[test]
    fn the_person_may_do_anything() {
        let c = Caller::person();
        for a in [Action::Delete, Action::ManageTokens, Action::Start] {
            assert_eq!(engine_check(&c, &a, &Target::Instance), Decision::Allow);
        }
    }

    #[test]
    fn read_tokens_only_read() {
        let c = Caller::api_token(Uuid::nil(), vec![TokenScope::Read]);
        assert_eq!(engine_check(&c, &Action::Read, &kind()), Decision::Allow);
        assert!(matches!(
            engine_check(&c, &Action::Start, &kind()),
            Decision::Forbidden(_)
        ));
    }

    #[test]
    fn run_and_admin_tokens_stop_where_their_scope_does() {
        let run = Caller::api_token(Uuid::nil(), vec![TokenScope::Run]);
        for a in [
            Action::Create,
            Action::Start,
            Action::Fork,
            Action::DeleteFork,
        ] {
            assert_eq!(engine_check(&run, &a, &kind()), Decision::Allow, "{a:?}");
        }
        for a in [Action::Delete, Action::ManageTokens, Action::ReplaceSecret] {
            assert!(
                matches!(engine_check(&run, &a, &kind()), Decision::Forbidden(_)),
                "{a:?}"
            );
        }
        let admin = Caller::api_token(Uuid::nil(), vec![TokenScope::Read, TokenScope::Admin]);
        assert_eq!(
            engine_check(&admin, &Action::Delete, &kind()),
            Decision::Allow
        );
        let branch = Target::Branch {
            run: "r",
            branch: "b",
        };
        assert!(matches!(
            engine_check(&admin, &Action::CallModel, &branch),
            Decision::Forbidden(_)
        ));
    }

    #[test]
    fn run_tokens_reach_only_their_branch() {
        let c = Caller::run_token("r", "b1", RunTokenKind::Run, None);
        let b1 = Target::Branch {
            run: "r",
            branch: "b1",
        };
        let b2 = Target::Branch {
            run: "r",
            branch: "b2",
        };
        assert_eq!(engine_check(&c, &Action::Finish, &b1), Decision::Allow);
        assert_eq!(engine_check(&c, &Action::Finish, &b2), Decision::NotFound);
        assert!(matches!(
            engine_check(&c, &Action::Read, &kind()),
            Decision::Forbidden(_)
        ));
    }

    fn worker() -> Caller {
        Caller::run_token("r", "b1", RunTokenKind::Worker, None).with_sandbox(Uuid::nil())
    }

    const B1: Target<'static> = Target::Branch {
        run: "r",
        branch: "b1",
    };
    const B2: Target<'static> = Target::Branch {
        run: "r",
        branch: "b2",
    };
    const RUN: Target<'static> = Target::Resource {
        kind: ResourceKind::Run,
        id: "r",
    };

    #[test]
    fn a_worker_token_links_and_calls_models_for_its_branch_only() {
        let c = worker();
        for a in [Action::WorkerLink, Action::CallModel] {
            assert_eq!(engine_check(&c, &a, &B1), Decision::Allow, "{a:?}");
            assert_eq!(engine_check(&c, &a, &B2), Decision::NotFound, "{a:?}");
            let d = engine_check(&c, &a, &RUN);
            assert!(matches!(d, Decision::Forbidden(_)), "{a:?}");
        }
    }

    #[test]
    fn a_worker_token_does_nothing_else() {
        use Action::*;
        let c = worker();
        let others = [
            Read,
            Create,
            Start,
            Fork,
            Stop,
            Record,
            UseInRun,
            Rename,
            ChangeSetup,
            ReplaceSecret,
            DeleteFork,
            Delete,
            ManageTokens,
            InstanceAdmin,
            RegisterAgent,
            Report,
            Finish,
            CallTools,
        ];
        for a in others {
            for t in [B1, B2, RUN, kind(), Target::Instance] {
                let d = engine_check(&c, &a, &t);
                assert!(matches!(d, Decision::Forbidden(_)), "{a:?} on {t:?}: {d:?}");
            }
        }
    }
}
