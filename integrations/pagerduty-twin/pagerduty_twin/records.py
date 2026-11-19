"""Reading the store: users (by name or email), services (by id, name or integration key), schedules
and incidents."""
from . import ids
from .errors import not_found


def user_by_name(db, name: str):
    return db.execute("SELECT * FROM users WHERE name = ?", (name,)).fetchone()


def user_by_email(db, email: str):
    return db.execute("SELECT * FROM users WHERE lower(email) = lower(?)", (email,)).fetchone()


def ensure_user(db, config: dict, name: str, now_ms: int, display_name: str | None = None, email: str | None = None):
    """The user called `name`, made now when it is not there yet."""
    row = user_by_name(db, name)
    if row is None:
        email = email or f"{name}@{config.get('email_domain', 'example.test')}"
        db.execute("INSERT INTO users(id, name, display_name, email, created_ms) VALUES (?, ?, ?, ?, ?)",
                   (ids.make("user", name), name, display_name or name, email, now_ms))
        row = user_by_name(db, name)
    return row


def users(db) -> list:
    return db.execute("SELECT * FROM users ORDER BY name").fetchall()


def services(db) -> list:
    return db.execute("SELECT * FROM services ORDER BY name").fetchall()


def service(db, ref: str):
    return db.execute("SELECT * FROM services WHERE id = ? OR name = ? OR integration_key = ?", (ref, ref, ref)).fetchone()


def schedules(db) -> list:
    return db.execute("SELECT * FROM schedules ORDER BY name").fetchall()


def schedule(db, schedule_id: str):
    return db.execute("SELECT * FROM schedules WHERE id = ?", (schedule_id,)).fetchone()


def incident(db, incident_id: str):
    number = ids.incident_number(incident_id)
    row = db.execute("SELECT * FROM incidents WHERE number = ?", (number,)).fetchone() if number else None
    if row is None:
        raise not_found()
    return row


def open_incident(db, service_id: str, dedup_key: str):
    return db.execute("SELECT * FROM incidents WHERE service_id = ? AND dedup_key = ? AND status != 'resolved'",
                      (service_id, dedup_key)).fetchone()


def notes(db, number: int) -> list:
    return db.execute("SELECT * FROM notes WHERE incident = ? ORDER BY id", (number,)).fetchall()
