from dataclasses import dataclass, field


@dataclass
class Run:
    name: str
    passed: int = 0
    failed: int = 0

    def total(self) -> int:
        return self.passed + self.failed
