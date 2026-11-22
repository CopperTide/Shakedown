//! Deploying a project experiment, and the deployments it has had.

use serde_json::{json, Value};
use shakedown_project::Id;

use super::{deploy, input, schema, Access, IdInput, OpCtx, OpError, OpFuture, Operation};
use crate::api::experiments::create_experiment;

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
        let (package, task) = deploy::runtime_of(&project, &id)?;
        let clock_start = match app.bench_named(&package).await {
            Ok(bench) => {
                (bench.tasks.iter().find(|t| t.id == task)).and_then(|t| t.clock_start.clone())
            }
            Err(_) => None,
        };
        let req = deploy::request(&project, &id, clock_start.as_deref())?;
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

fn internal(e: anyhow::Error) -> OpError {
    OpError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        format!("{e:#}"),
    )
}
