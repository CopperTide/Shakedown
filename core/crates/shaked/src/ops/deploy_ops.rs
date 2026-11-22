//! Deploying a project experiment, and the deployments it has had.

use serde_json::{json, Value};
use shakedown_project::Id;

use std::sync::Arc;

use super::{deploy, input, schema, Access, IdInput, OpCtx, OpError, OpFuture, Operation};
use crate::agents::endpoint_auth;
use crate::api::experiments::create_experiment;
use crate::app::AppState;
use crate::auth::Caller;
use crate::providers::keys::provider_refusal;

pub(super) fn ops() -> Vec<Operation> {
    vec![
        Operation {
            name: "experiments.deploy",
            area: "Experiments",
            summary: "Runs the experiment: every setup the number of times it says, on the world's runtime",
            access: Access::Spend,
            method: "POST",
            path: "/api/project/experiments/{id}/deploy",
            target: Some(shakedown_project::format::Kind::Experiment),
            input_schema: schema::<IdInput>,
            run: deploy_one,
        },
        Operation {
            name: "experiments.runs",
            area: "Experiments",
            summary: "The experiment's deployments, newest first, with each setup's results",
            access: Access::Read,
            method: "GET",
            path: "/api/project/experiments/{id}/runs",
            target: Some(shakedown_project::format::Kind::Experiment),
            input_schema: schema::<IdInput>,
            run: deployments,
        },
    ]
}

fn deploy_one<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let id = Id::parse(&input::<IdInput>(value)?.id)?;
        let app = ctx.app("deploying")?;
        let project = ctx.project.current()?;
        let creds = credentials(app, &ctx.caller, &deploy::needs(&project, &id)?).await?;
        let req = deploy::request(&project, &id, &creds)?;
        let detail = create_experiment(app, &ctx.caller, None, req).await?;
        serde_json::to_value(detail).map_err(|e| OpError::bad_request(e.to_string()))
    })
}

fn deployments<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let id = Id::parse(&input::<IdInput>(value)?.id)?;
        let app = ctx.app("listing deployments")?;
        let records = app.store.at(&ctx.caller);
        let mut out = Vec::new();
        for exp in records
            .deployments_of(id.as_str())
            .await
            .map_err(internal)?
        {
            if let Some(stored) = records.experiment(&exp).await.map_err(internal)? {
                let detail = crate::experiment::detail(app.store.at(&ctx.caller), &stored)
                    .await
                    .map_err(internal)?;
                out.push(
                    serde_json::to_value(detail)
                        .map_err(|e| OpError::bad_request(e.to_string()))?,
                );
            }
        }
        Ok(json!({ "deployments": out }))
    })
}

/// What the experiment's setups name, looked up for the caller: which keys may serve a built-in
/// agent and which credentials an endpoint may be sent. Nothing is looked up when no setup names
/// one; no secret is kept.
async fn credentials(
    app: &Arc<AppState>,
    caller: &Caller,
    needs: &deploy::Needs,
) -> Result<deploy::Credentials, OpError> {
    let mut creds = deploy::Credentials::default();
    if !needs.providers.is_empty() {
        let rows = app.store.at(caller).list_keys().await.map_err(internal)?;
        let ids: Vec<_> = rows.iter().map(|r| r.id).collect();
        let mine = app
            .store
            .layer()
            .own_keys(caller, &ids)
            .await
            .map_err(internal)?;
        for name in &needs.providers {
            let found = rows.iter().find(|r| &r.name == name);
            let verdict = match found {
                None => Err("is not stored".to_string()),
                Some(r) => match provider_refusal(r, mine.contains(&r.id)) {
                    Some(why) => Err(why),
                    None => Ok(()),
                },
            };
            creds.provider(name, verdict);
        }
    }
    for auth in &needs.auth {
        let read = endpoint_auth::resolve(app, caller, auth).await;
        creds.credential(auth.clone(), read.map(|_| ()));
    }
    Ok(creds)
}

fn internal(e: anyhow::Error) -> OpError {
    OpError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        format!("{e:#}"),
    )
}
