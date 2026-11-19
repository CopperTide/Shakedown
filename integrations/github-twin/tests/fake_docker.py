"""A Docker for tests: records every command line and answers by the first rule whose text the
command contains (exit code, output), else success."""
import threading

from github_twin.actions.runner import Completed


class FakeDocker:
    def __init__(self):
        self.calls: list[list[str]] = []
        self.rules: list[tuple[str, int, str]] = []
        self.gate: threading.Event | None = None
        self.entered = threading.Event()

    def answer(self, contains: str, code: int, output: str = "") -> None:
        self.rules.append((contains, code, output))

    def hold(self) -> threading.Event:
        """Make the next commands wait until the returned event is set (a job still running)."""
        self.gate = threading.Event()
        return self.gate

    def run(self, argv, timeout_s=None, cancelled=lambda: False) -> Completed:
        self.calls.append(list(argv))
        self.entered.set()
        while self.gate is not None and not self.gate.wait(0.05):
            if cancelled():
                return Completed(137, "cancelled\n")
        line = " ".join(argv)
        for contains, code, output in self.rules:
            if contains in line:
                return Completed(code, output)
        return Completed(0, "ok\n")
