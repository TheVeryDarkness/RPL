import dataclasses
from pathlib import Path
import os
from typing import Iterable
import subprocess
import sys
import shutil


@dataclasses.dataclass
class CVE:
    """
    Represents a CVE entry for evaluation.

    Attributes:
        cve_id: The ID of the CVE (e.g., "CVE-2018-21000").
        repo: The URL of the crate's repository.
        sha_before: The git SHA of the crate before the fix for the CVE.
        sha_after: The git SHA of the crate after the fix for the CVE.
    """

    cve_id: str
    sha_before: str
    sha_after: str
    base: str
    patched: list[str]
    cross_stmt: list[str]
    cross_fn: list[str]
    desc: str = ""

    def files(self) -> Iterable[str]:
        yield self.base
        yield from self.patched
        yield from self.cross_stmt
        yield from self.cross_fn


@dataclasses.dataclass
class Crate:
    """
    Represents a Rust crate for evaluation.

    Attributes:
        name: The name of the crate (e.g., "safe-transmute").
        repo: The URL of the crate's repository.
        cves: A list of CVE entries associated with the crate.
    """

    name: str
    repo: str
    cves: list[CVE]


crates = [
    Crate(
        name="safe-transmute",
        repo="https://github.com/nabijaczleweli/safe-transmute-rs",
        cves=[
            CVE(
                cve_id="CVE-2018-21000",
                sha_before="c79ebfdb5858982af59a78df471c7cad7a78fd23",
                sha_after="a134e06d740f9d7c287f74c0af2cd06206774364",
                base="cve_2018_21000/simplified.rs",
                patched=["cve_2018_21000/simplified_patched.rs"],
                cross_stmt=[],
                cross_fn=[
                    "cve_2018_21000/simplified_cross_fn.rs",
                    "cve_2018_21000/simplified_cross_fn_1.rs",
                    "cve_2018_21000/simplified_cross_fn_2.rs",
                ],
                desc="Already crossing statements",
            )
        ],
    ),
    Crate(
        name="image",
        repo="https://github.com/image-rs/image",
        cves=[
            CVE(
                cve_id="CVE-2019-16138",
                sha_before="5c51a7be311677c53ebbb2f66f61e64714ab33c7",
                sha_after="65d17446c4242da0f9e1ae84b9dbce5108a822f5",
                base="cve_2019_16138/simplified.rs",
                patched=["cve_2019_16138/simplified_patched.rs"],
                cross_stmt=[],
                cross_fn=[
                    "cve_2019_16138/simplified_cross_fn_1.rs",
                    "cve_2019_16138/simplified_cross_fn_2.rs",
                ],
                desc="Already crossing statements",
            )
        ],
    ),
    Crate(
        name="rusqlite",
        repo="https://github.com/rusqlite/rusqlite",
        cves=[
            CVE(
                cve_id="CVE-2020-35873",
                sha_before="552416039ec700471167c22f32e07d7d2126cea1",
                sha_after="ac30e169ae51b262bc8cf7026469851ce39b23c6",
                base="cve_2020_35873/cve_2020_35873.rs",
                patched=["cve_2020_35873/cve_2020_35873_patched.rs"],
                cross_stmt=[],
                cross_fn=[],
                desc="Already crossing functions",
            )
        ],
    ),
    Crate(
        name="destructure_traitobject",
        repo="https://github.com/philip-peterson/destructure_traitobject",
        cves=[
            CVE(
                cve_id="CVE-2020-35881",
                sha_before="8aabddab131f84daa3ba5da9a9799c55efc32403",
                sha_after="99b1993a13bf80e93031048586526384d1d8bddc",
                base="cve_2020_35881/cve_2020_35881.rs",
                patched=["cve_2020_35881/cve_2020_35881_patched.rs"],
                cross_stmt=[],
                cross_fn=[],
                desc="Quite basic, thus cannot be split further",
            )
        ],
    ),
    Crate(
        name="arr",
        repo="https://github.com/sjep/array",
        cves=[
            CVE(
                cve_id="CVE-2020-35886",
                sha_before="efa214159eaad2abda7b072f278d678f8788c307",
                sha_after="34380d886168061149af0ee47fe07a74b7ebf4e2",
                base="cve_2020_35886/cve_2020_35886.rs",
                patched=["cve_2020_35886/cve_2020_35886_patched.rs"],
                cross_stmt=[],
                cross_fn=["cve_2020_35886/cve_2020_35886_cross_fn.rs"],
            ),
            CVE(
                cve_id="CVE-2020-35887",
                sha_before="efa214159eaad2abda7b072f278d678f8788c307",
                sha_after="34380d886168061149af0ee47fe07a74b7ebf4e2",
                base="cve_2020_35887/cve_2020_35887.rs",
                patched=["cve_2020_35887/cve_2020_35887_patched.rs"],
                cross_stmt=["cve_2020_35887/cve_2020_35887_cross_stmt.rs"],
                cross_fn=["cve_2020_35887/cve_2020_35887_cross_fn.rs"],
            ),
            CVE(
                cve_id="CVE-2020-35888",
                sha_before="efa214159eaad2abda7b072f278d678f8788c307",
                sha_after="34380d886168061149af0ee47fe07a74b7ebf4e2",
                base="cve_2020_35888/cve_2020_35888_simplified.rs",
                patched=["cve_2020_35888/cve_2020_35888_patched.rs"],
                cross_stmt=["cve_2020_35888/cve_2020_35888_cross_stmt.rs"],
                cross_fn=["cve_2020_35888/cve_2020_35888_cross_fn.rs"],
            ),
        ],
    ),
    Crate(
        name="actix-utils",
        repo="https://github.com/actix/actix-net",
        cves=[
            CVE(
                cve_id="CVE-2020-35898",
                sha_before="5d6d309e66b79d837ede20a26d654274bfb68d8f",
                sha_after="0dca1a705ad1ff4885b3491ecb809a808e1de66c",
                base="cve_2020_35898_9/cve_2020_35898_9.rs",
                patched=["cve_2020_35898_9/cve_2020_35898_9_patched.rs"],
                cross_stmt=["cve_2020_35898_9/cve_2020_35898_9_cross_stmt.rs"],
                cross_fn=["cve_2020_35898_9/cve_2020_35898_9_cross_fn.rs"],
            )
        ],
    ),
    Crate(
        name="actix-http",
        repo="https://github.com/actix/actix-web",
        cves=[
            CVE(
                cve_id="CVE-2020-35901",
                sha_before="3033f187d2214fdb40c815d982fbf6f8a31bcd3f",
                sha_after="fe13789345f6307e8ac1e1545770f82a14b6588b",
                base="cve_2020_35901_2/cve_2020_35901.rs",
                patched=["cve_2020_35901_2/cve_2020_35901_patched.rs"],
                cross_stmt=["cve_2020_35901_2/cve_2020_35901_cross_stmt.rs"],
                cross_fn=["cve_2020_35901_2/cve_2020_35901_cross_fn.rs"],
            )
        ],
    ),
    Crate(
        name="actix-codec",
        repo="https://github.com/actix/actix-net",
        cves=[
            CVE(
                cve_id="CVE-2020-35902",
                sha_before="693d5132a944b260b68071b938b6b2e532a7982c",
                sha_after="c41b5d8dd4235ccca84d0b687996615c0c64d956",
                base="cve_2020_35901_2/cve_2020_35902.rs",
                patched=["cve_2020_35901_2/cve_2020_35902_patched.rs"],
                cross_stmt=["cve_2020_35901_2/cve_2020_35902_cross_stmt.rs"],
                cross_fn=[],
            )
        ],
    ),
    Crate(
        name="futures-task",
        repo="https://github.com/rust-lang/futures-rs",
        cves=[
            CVE(
                cve_id="CVE-2020-35907",
                sha_before="18bf8738dfd9f9f7fea6a308a45f763402dff8cb",
                sha_after="d98e6ecb231988ba0d017870ee95ca51a2c7315c",
                base="cve_2020_35907/cve_2020_35907.rs",
                patched=["cve_2020_35907/cve_2020_35907_patched.rs"],
                cross_stmt=[],
                cross_fn=["cve_2020_35907/cve_2020_35907_cross_fn.rs"],
            )
        ],
    ),
    Crate(
        name="bra",
        repo="https://github.com/Enet4/bra-rs",
        cves=[
            CVE(
                cve_id="CVE-2021-25905",
                sha_before="ff2b2995f43c24193aa71a3801bf7af287e2ca98",
                sha_after="aabf5562f8c6374ab30f615b28e0cff9b5c79e5f",
                base="cve_2021_25905/greedy.rs",
                patched=["cve_2021_25905/greedy_patched.rs"],
                cross_stmt=["cve_2021_25905/greedy_cross_stmt.rs"],
                cross_fn=["cve_2021_25905/greedy_cross_fn.rs"],
            )
        ],
    ),
    Crate(
        name="ash",
        repo="https://github.com/ash-rs/ash",
        cves=[
            CVE(
                cve_id="CVE-2021-45688",
                sha_before="17149bd791cd6e09b145063a238d3e27d855780c",
                sha_after="2c98b6f384a017de031698bd623551a45f24c8f9",
                base="cve_2021_45688/cve_2021_45688.rs",
                patched=["cve_2021_45688/cve_2021_45688_patched.rs"],
                cross_stmt=["cve_2021_45688/cve_2021_45688_cross_stmt.rs"],
                cross_fn=["cve_2021_45688/cve_2021_45688_cross_fn.rs"],
            )
        ],
    ),
    Crate(
        name="crossbeam-utils",
        repo="https://github.com/crossbeam-rs/crossbeam",
        cves=[
            CVE(
                cve_id="CVE-2022-23639",
                sha_before="be6ff29e1fce196519b65cb0b647f1ad72659498",
                sha_after="f7c378b26e273d237575154800f6c2bd3bf20058",
                base="cve_2022_23639/cve_2022_23639.rs",
                patched=["cve_2022_23639/cve_2022_23639_patched.rs"],
                cross_stmt=["cve_2022_23639/cve_2022_23639_cross_stmt.rs"],
                cross_fn=["cve_2022_23639/cve_2022_23639_cross_fn.rs"],
            )
        ],
    ),
]


target = Path(os.path.expanduser("~/cve_evaluation"))
print(f"Will storing results in {target}, is it ok? (y/n)")
if input().strip().lower() != "y":
    print("Aborting.")
    sys.exit(1)
if target.exists():
    shutil.rmtree(target)
target.mkdir(parents=True, exist_ok=True)

# Verify that all files exist
for crate in crates:
    for cve in crate.cves:
        for file in cve.files():
            path = Path(os.path.dirname(__file__), "../../../tests/ui/cve/", file)
            assert (
                path.exists()
            ), f"File {file} (resolved to {path}) does not exist for CVE {cve.cve_id} in crate {crate.name}"

lib = Path(target, "lib")
lib.mkdir(parents=True, exist_ok=True)

print(lib)

cargo_toml = Path(lib, "Cargo.toml")
cargo_toml.write_text(
    """
[workspace]

[package]
name = "cve_evaluation"
version = "0.1.0"
edition = "2021"

[dependencies]
bytes = "0.5.6"
libc = "0.2"
log = "0.4"
tokio = { version = "0.2.25", features = ["io-util"] }
tokio-util = { version = "0.2", features = ["codec"] }
futures = "0.3"
pin-project = "1.0"
"""
)

# Install required Rust toolchains and components for rapx
subprocess.run(
    [
        "rustup",
        "component",
        "add",
        "--toolchain",
        "nightly-2025-12-06",
        "rust-src",
        "rustc-dev",
        "llvm-tools-preview",
    ],
    check=True,
)
subprocess.run(
    ["cargo", "+nightly-2025-12-06", "install", "rapx@0.6.252"],
    check=True,
)


def store(result: "subprocess.CompletedProcess[str]", tool: str, file: str):
    stderr = Path(target, "log", tool, file).with_suffix(".stderr")
    stderr.parent.mkdir(parents=True, exist_ok=True)
    stderr.write_text(result.stderr)

    stdout = Path(target, "log", tool, file).with_suffix(".stdout")
    stdout.parent.mkdir(parents=True, exist_ok=True)
    stdout.write_text(result.stdout)


# Check all files with tools to ensure they compile without errors
for crate in crates:
    for cve in crate.cves:
        for file in cve.files():
            path = Path(os.path.dirname(__file__), "../../../tests/ui/cve/", file)
            print(f"Checking {path} for CVE {cve.cve_id} in crate {crate.name}...")
            lib_rs = Path(lib, "src/lib.rs")
            lib_rs.parent.mkdir(parents=True, exist_ok=True)
            lib_rs.write_text(path.read_text())

            # cargo clippy
            result = subprocess.run(
                ["cargo", "clippy"],
                cwd=lib,
                capture_output=True,
                text=True,
            )
            store(result, "clippy", file)

            # cargo rudra
            result = subprocess.run(
                ["cargo", "+nightly-2025-02-14", "rudra"],
                cwd=lib,
                capture_output=True,
                text=True,
            )
            store(result, "rudra", file)

            # cargo rapx
            result = subprocess.run(
                ["cargo", "+nightly-2025-12-06", "rapx", "-F", "-M"],
                cwd=lib,
                capture_output=True,
                text=True,
            )
            store(result, "rapx", file)
