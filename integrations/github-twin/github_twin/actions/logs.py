"""The logs runs keep: a job's whole log as text, a run's as the zip `gh run view --log` reads
(`<job>/<n>_<step>.txt` and `<k>_<job>.txt`), and the signed `/_logs/...` URLs the API redirects to
(a client may drop its token on a redirect, as it would to GitHub's log storage)."""
import hashlib
import hmac
import io
import zipfile
from datetime import datetime, timezone

from arena_part.twin import Response

from . import jobs

TEXT, ZIP = "text/plain; charset=utf-8", "application/zip"


def safe(name: str) -> str:
    return name.replace("/", "_")


def job_text(steps: list) -> str:
    return "".join(f"##[group]{s['name']}\n{s['log']}##[endgroup]\n" for s in steps)


def _info(path: str, ms: int | None) -> zipfile.ZipInfo:
    t = datetime.fromtimestamp((ms or 315_532_800_000) // 1000, tz=timezone.utc)
    return zipfile.ZipInfo(path, (max(t.year, 1980), t.month, t.day, t.hour, t.minute, t.second))


def run_zip(db, run) -> bytes:
    """The logs of the run's latest attempt, a zip (dated in world time, so two equal runs give equal bytes)."""
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for k, job in enumerate(jobs.jobs_of(db, run["id"], run["run_attempt"])):
            steps, name = jobs.steps_of(db, job["id"]), safe(job["name"])
            z.writestr(_info(f"{k}_{name}.txt", job["completed_ms"]), job_text(steps))
            for s in steps:
                z.writestr(_info(f"{name}/{s['number']}_{safe(s['name'])}.txt", s["completed_ms"]), s["log"])
    return out.getvalue()


def sign(token: str, what: str) -> str:
    return hmac.new(token.encode(), what.encode(), hashlib.sha256).hexdigest()[:40]


def url(v, what: str) -> str:
    """The signed address of `what` (`jobs/<id>` or `runs/<id>`) under the request's public base."""
    return f"{v.urls.html(f'/_logs/{what}')}?sig={sign(v.config.get('token', ''), what)}"


def _signed(req, ctx, kind: str) -> int | None:
    what = f"{kind}/{req.params['id']}"
    ok = hmac.compare_digest(req.arg("sig") or "", sign(ctx.config.get("token", ""), what))
    return int(req.params["id"]) if ok and req.params["id"].isdigit() else None


def serve_job(req, ctx) -> Response:
    job_id = _signed(req, ctx, "jobs")
    if job_id is None or jobs.job_row(ctx.store.conn, job_id) is None:
        return Response(404, "Not Found\n")
    return Response(200, job_text(jobs.steps_of(ctx.store.conn, job_id)), headers={"Content-Type": TEXT})


def serve_run(req, ctx) -> Response:
    run_id = _signed(req, ctx, "runs")
    run = jobs.run_row(ctx.store.conn, run_id) if run_id is not None else None
    if run is None:
        return Response(404, "Not Found\n")
    return Response(200, run_zip(ctx.store.conn, run), headers={"Content-Type": ZIP})
