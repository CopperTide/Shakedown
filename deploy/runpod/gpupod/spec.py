"""The pod, as ``pod.toml`` describes it: image, disks, GPUs in order, vLLM's arguments, limits."""

from __future__ import annotations

import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

POD_TOML = Path(__file__).resolve().parent.parent / "pod.toml"


@dataclass(frozen=True)
class Gpu:
    id: str
    model: str
    revision: str


@dataclass(frozen=True)
class PodSpec:
    name: str
    image: str
    min_cuda: str
    cloud: str
    container_disk_gb: int
    volume_gb: int
    volume_mount: str
    port: int
    idle_minutes: int
    max_session_hours: float
    budget_usd: float
    arenad_provider: str
    hosted_price_per_mtok: tuple[float, float]
    serve_args: tuple[str, ...]
    low_memory_args: tuple[str, ...]
    env: dict
    gpus: tuple[Gpu, ...]

    def gpu(self, gpu_id: Optional[str] = None) -> Gpu:
        """The GPU ``gpu_id`` of the list (default: the first)."""
        if gpu_id is None:
            return self.gpus[0]
        for g in self.gpus:
            if g.id == gpu_id:
                return g
        raise ValueError(f"{gpu_id} is not in pod.toml; it lists {', '.join(g.id for g in self.gpus)}")

    def after(self, gpu: Gpu) -> list[Gpu]:
        """The GPUs after ``gpu`` in order of preference."""
        return list(self.gpus[self.gpus.index(gpu) + 1:])


def load_spec(path: Path = POD_TOML) -> PodSpec:
    d = tomllib.loads(path.read_text())
    gpus = tuple(Gpu(**g) for g in d.pop("gpu"))
    return PodSpec(**{**d, "gpus": gpus, "hosted_price_per_mtok": tuple(d["hosted_price_per_mtok"]),
                      "serve_args": tuple(d["serve_args"]),
                      "low_memory_args": tuple(d["low_memory_args"])})


def proxy_base(pod_id: str, port: int) -> str:
    """The pod's public HTTPS proxy for ``port`` (RunPod: ``https://<pod>-<port>.proxy.runpod.net``)."""
    return f"https://{pod_id}-{port}.proxy.runpod.net"
