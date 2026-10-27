from dataclasses import dataclass, field


@dataclass
class Run:
    name: str
    passed: int = 0
    failed: int = 0
    skipped: int = 0
    tags: list[str] = field(default_factory=list)

    def total(self) -> int:
        return self.passed + self.failed
