"""Users, services, schedules, on-calls, incidents and notes as the REST API answers them; every
object and reference has an absolute `self` from the public base."""
import json

from . import ids, oncall, records
from .times import iso

KINDS = {"user": "users", "service": "services", "schedule": "schedules", "escalation_policy": "escalation_policies",
         "incident": "incidents"}


class Shapes:
    def __init__(self, db, base: str):
        self.db, self.base = db, base

    def ref(self, kind: str, id_: str, summary: str) -> dict:
        path = f"{KINDS[kind]}/{id_}"
        return {"id": id_, "type": f"{kind}_reference", "summary": summary, "self": f"{self.base}/{path}",
                "html_url": f"{self.base}/{path}"}

    def user_ref(self, name: str) -> dict:
        row = records.user_by_name(self.db, name)
        return self.ref("user", row["id"], row["display_name"])

    def service_ref(self, row) -> dict:
        return self.ref("service", row["id"], row["name"])

    def policy_ref(self, service) -> dict:
        return self.ref("escalation_policy", service["policy_id"], f"{service['name']} policy")

    def schedule_ref(self, row) -> dict:
        return self.ref("schedule", row["id"], row["name"])

    def user(self, row) -> dict:
        return {**self.ref("user", row["id"], row["display_name"]), "type": "user", "name": row["display_name"],
                "email": row["email"], "time_zone": "Etc/UTC", "color": "purple", "role": "user",
                "avatar_url": f"{self.base}/avatars/{row['id']}", "description": "", "invitation_sent": False,
                "job_title": "", "teams": [], "contact_methods": [], "notification_rules": []}

    def service(self, row) -> dict:
        busy = self.db.execute("SELECT status FROM incidents WHERE service_id = ? AND status != 'resolved'",
                               (row["id"],)).fetchall()
        states = {r[0] for r in busy}
        status = "critical" if "triggered" in states else "warning" if states else "active"
        last = self.db.execute("SELECT MAX(created_ms) FROM incidents WHERE service_id = ?", (row["id"],)).fetchone()[0]
        out = {**self.service_ref(row), "type": "service", "name": row["name"], "description": "",
               "auto_resolve_timeout": 14400, "acknowledgement_timeout": 1800, "created_at": iso(row["created_ms"]),
               "status": status, "escalation_policy": self.policy_ref(row), "teams": [], "integrations": [],
               "alert_creation": "create_alerts_and_incidents"}
        if last:
            out["last_incident_timestamp"] = iso(last)
        return out

    def schedule(self, row) -> dict:
        users = [self.user_ref(n) for n in dict.fromkeys(json.loads(row["rotation"]))]
        policies = [self.policy_ref(s) for s in self.db.execute("SELECT * FROM services WHERE schedule_id = ? "
                                                                "ORDER BY name", (row["id"],))]
        return {**self.schedule_ref(row), "type": "schedule", "name": row["name"], "time_zone": "Etc/UTC",
                "description": "", "users": users, "escalation_policies": policies, "teams": []}

    def oncall(self, service, schedule, at_ms: int) -> dict | None:
        s = oncall.shift(schedule, at_ms)
        return None if s is None else {
            "escalation_policy": self.policy_ref(service), "user": self.user_ref(s.user),
            "schedule": self.schedule_ref(schedule), "escalation_level": 1, "start": iso(s.start_ms),
            "end": iso(s.end_ms)}

    def _by(self, who: str | None) -> dict | None:
        kind, _, ref = (who or "").partition(":")
        if kind == "user":
            return self.user_ref(ref)
        row = records.service(self.db, ref) if kind == "service" else None
        return self.service_ref(row) if row else None

    def _people(self, row) -> dict:
        assignments = [{"at": iso(row["created_ms"]), "assignee": self.user_ref(row["assignee"])}] \
            if row["assignee"] and row["status"] != "resolved" else []
        acks = [{"at": iso(row["acked_ms"]), "acknowledger": self._by(row["acked_by"])}] \
            if row["status"] == "acknowledged" else []
        return {"assignments": assignments, "acknowledgements": acks}

    def incident(self, row) -> dict:
        """No `priority` or `resolve_reason` (the account has neither); `resolved_at` once resolved."""
        service = records.service(self.db, row["service_id"])
        number, resolved = row["number"], row["status"] == "resolved"
        out = {**self.ref("incident", ids.incident(number), f"[#{number}] {row['title']}"), "type": "incident",
                "incident_number": number, "title": row["title"], "created_at": iso(row["created_ms"]),
                "updated_at": iso(row["updated_ms"]), "status": row["status"], "incident_key": row["dedup_key"],
                "service": self.service_ref(service), **self._people(row),
                "last_status_change_at": iso(row["changed_ms"]), "last_status_change_by": self._by(row["changed_by"]),
                "escalation_policy": self.policy_ref(service), "teams": [], "pending_actions": [],
                "urgency": "high" if row["severity"] in ("critical", "error") else "low", "is_mergeable": True,
                "assigned_via": "escalation_policy", "incidents_responders": [], "responder_requests": [],
                "alert_counts": {"all": row["alerts"], "triggered": 0 if resolved else row["alerts"],
                                 "resolved": row["alerts"] if resolved else 0}}
        if resolved:
            out["resolved_at"] = iso(row["resolved_ms"])
        return out

    def note(self, row) -> dict:
        return {"id": f"PN{row['id']}", "user": self.user_ref(row["user"]),
                "channel": {"summary": "The PagerDuty website or APIs"}, "content": row["content"],
                "created_at": iso(row["created_ms"])}
