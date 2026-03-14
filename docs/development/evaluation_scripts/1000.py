import dataclasses
from math import nan
from pathlib import Path
import os
from time import time
import subprocess
import sys
import shutil
import toml


@dataclasses.dataclass
class Crate:
    """
    Represents a Rust crate for evaluation.

    Attributes:
        name: The name of the crate (e.g., "safe-transmute").
        version: The version of the crate (e.g., "0.1.0").
    """

    name: str
    version: str


crates_toml = toml.load(
    Path(os.path.dirname(__file__), "../../../crates/lintcheck/1000.toml")
)
print(crates_toml)
crates = [
    Crate(name=crate["name"], version=crate["version"])
    for _, crate in crates_toml["crates"].items()
]

subprocess.run(["cargo", "install", "cargo-download@0.1.2"], check=True)

source = Path(os.path.expanduser("~/lintcheck/sources"))
target = Path(os.path.expanduser("~/crates_evaluation_1000"))
print(f"Will storing results in {target}, is it ok? (y/n)")
if input().strip().lower() != "y":
    print("Aborting.")
    sys.exit(1)
if target.exists():
    shutil.rmtree(target)
target.mkdir(parents=True, exist_ok=True)


def run(
    command: list[str], lib: Path, tool: str, crate: str, version: str, f: Path
) -> tuple[float, int]:
    time1 = time()
    try:
        result = subprocess.run(
            command,
            cwd=lib,
            capture_output=True,
            text=True,
            timeout=60,  # 1 minute
        )
        time2 = time()
        dtime = time2 - time1
    except subprocess.TimeoutExpired:
        result = subprocess.CompletedProcess(command, -1, "", "Timed out")
        dtime = nan
        print(f"{tool} timed out on {crate} {version}")
    store(result, tool, f"{crate}-{version}")
    f.open("a").write(f"{crate},{version},{tool},{dtime},{result.returncode}\n")
    return dtime, result.returncode


def clean(lib: Path):
    subprocess.run(
        ["cargo", "clean"],
        cwd=lib,
        text=True,
        check=True,
    )


def store(result: "subprocess.CompletedProcess[str]", tool: str, file: str):
    stderr = Path(target, "log", tool, file).with_suffix(".stderr")
    stderr.parent.mkdir(parents=True, exist_ok=True)
    stderr.write_text(result.stderr)

    stdout = Path(target, "log", tool, file).with_suffix(".stdout")
    stdout.parent.mkdir(parents=True, exist_ok=True)
    stdout.write_text(result.stdout)


records = Path(target, "results.csv")
records.write_text("crate,version,tool,time,exit_code\n")

for crate in crates:
    print(f"Processing {crate.name} {crate.version}...")

    lib = source / f"{crate.name}-{crate.version}"

    run(
        ["cargo", "check"],
        lib,
        "rustc",
        crate.name,
        crate.version,
        records,
    )
    clean(lib)

    # cargo rpl
    run(
        ["cargo", "+nightly-2025-02-14", "rpl"],
        lib,
        "rpl",
        crate.name,
        crate.version,
        records,
    )
    clean(lib)

    # cargo clippy
    run(["cargo", "clippy"], lib, "clippy", crate.name, crate.version, records)
    clean(lib)

    # cargo rudra
    run(
        ["cargo", "+nightly-2025-02-14", "rudra"],
        lib,
        "rudra",
        crate.name,
        crate.version,
        records,
    )
    clean(lib)

    # cargo rapx
    run(
        ["cargo", "+nightly-2025-12-06", "rapx", "-F", "-M"],
        lib,
        "rapx",
        crate.name,
        crate.version,
        records,
    )
    clean(lib)
