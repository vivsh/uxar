#!/usr/bin/env python3
from pathlib import Path

FILES = [
    "src/bin/t1_vyuh.rs",
    "src/bin/t1_axum.rs",
    "src/bin/t1_rocket.rs",
    "src/bin/t1_actix.rs",
    "python/t1_fastapi.py",
    "src/bin/t2_vyuh.rs",
    "src/bin/t2_axum.rs",
    "src/bin/t2_rocket.rs",
    "src/bin/t2_actix.rs",
    "python/t2_fastapi.py",
]

FOLDERS = [
    "python/drf_t1",
    "python/drf_t2",
]


def count(path: Path) -> int:
    lines = 0
    for raw in path.read_text().splitlines():
        line = raw.strip()
        if not line:
            continue
        if line.startswith("//") or line.startswith("#"):
            continue
        lines += 1
    return lines


def count_folder(path: Path) -> int:
    return sum(count(file) for file in sorted(path.rglob("*.py")))


def main() -> None:
    root = Path(__file__).resolve().parents[1]
    print("| implementation | loc |")
    print("| --- | ---: |")
    for rel in FILES:
        path = root / rel
        value = count(path) if path.exists() else 0
        print(f"| `{rel}` | {value} |")
    for rel in FOLDERS:
        path = root / rel
        value = count_folder(path) if path.exists() else 0
        print(f"| `{rel}/` | {value} |")


if __name__ == "__main__":
    main()
