"""The hourly rate of a GPU right now, from ``runpodctl gpu list`` (pod on-demand prices per
cloud; a ``null`` price means that cloud does not offer the GPU, never that it is free)."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Callable, Optional

from .runpodctl import Runner, call

SOURCE = "runpodctl gpu list"


@dataclass(frozen=True)
class Quote:
    gpu_id: str
    cloud: str
    usd_h: Optional[float]
    available: bool
    source: str
    read_at: str

    def entry(self) -> dict:
        return {"kind": "quote", "gpu_id": self.gpu_id, "cloud": self.cloud, "usd_h": self.usd_h,
                "available": self.available, "source": self.source, "read_at": self.read_at}


def utc_now() -> datetime:
    return datetime.now(timezone.utc)


def current_rate(runner: Runner, gpu_id: str, cloud: str,
                 now: Callable[[], datetime] = utc_now) -> Quote:
    gpus = call(runner, "gpu", "list", "--include-unavailable")
    g = next((x for x in gpus if x.get("gpuId") == gpu_id), {})
    price = g.get("communityPricePerHr" if cloud == "COMMUNITY" else "securePricePerHr")
    usd_h = float(price) if price is not None else None
    available = bool(g.get("available")) and usd_h is not None
    return Quote(gpu_id, cloud, usd_h, available, SOURCE, now().strftime("%Y-%m-%dT%H:%MZ"))


def quote_line(q: Quote, spent: float, budget: float, hours: float) -> str:
    where = f"{q.gpu_id}, {q.cloud.lower()}"
    if q.usd_h is None:
        return f"{where}: not offered ({q.source}, {q.read_at}). Spent so far ${spent:.2f} of ${budget:.2f}."
    stock = "" if q.available else " (no stock now)"
    return (f"{where}: ${q.usd_h:.2f}/h{stock} ({q.source}, {q.read_at}). "
            f"Spent so far ${spent:.2f} of ${budget:.2f}; {hours:g} h at this rate would bring it to "
            f"${spent + q.usd_h * hours:.2f}.")
