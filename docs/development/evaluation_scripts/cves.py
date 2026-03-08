import dataclasses
import re
import os
import sys
import toml
import glob


@dataclasses.dataclass
class CVE:
    """
    Represents a CVE entry for evaluation.

    Attributes:
        cve_id: The ID of the CVE (e.g., "CVE-2018-21000").
        crate_name: The name of the crate affected by the CVE (e.g., "safe-transmute").
        repo: The URL of the crate's repository.
        sha_before: The git SHA of the crate before the fix for the CVE.
        sha_after: The git SHA of the crate after the fix for the CVE.
    """

    cve_id: str | list[str]
    crate_name: str
    repo: str
    sha_before: str
    sha_after: str


cves = [
    CVE(
        cve_id="CVE-2018-21000",
        crate_name="safe-transmute",
        repo="https://github.com/nabijaczleweli/safe-transmute-rs",
        sha_before="c79ebfdb5858982af59a78df471c7cad7a78fd23",
        sha_after="a134e06d740f9d7c287f74c0af2cd06206774364",
    ),
    CVE(
        cve_id="CVE-2019-16138",
        crate_name="image",
        repo="https://github.com/image-rs/image",
        sha_before="5c51a7be311677c53ebbb2f66f61e64714ab33c7",
        sha_after="65d17446c4242da0f9e1ae84b9dbce5108a822f5",
    ),
    CVE(
        cve_id="CVE-2020-35873",
        crate_name="rusqlite",
        repo="https://github.com/rusqlite/rusqlite",
        sha_before="552416039ec700471167c22f32e07d7d2126cea1",
        sha_after="ac30e169ae51b262bc8cf7026469851ce39b23c6",
    ),
    CVE(
        cve_id="CVE-2020-35881",
        crate_name="destructure_traitobject",
        repo="https://github.com/philip-peterson/destructure_traitobject",
        sha_before="8aabddab131f84daa3ba5da9a9799c55efc32403",
        sha_after="99b1993a13bf80e93031048586526384d1d8bddc",
    ),
    CVE(
        cve_id=["CVE-2020-35886", "CVE-2020-35887", "CVE-2020-35888"],
        crate_name="arr",
        repo="https://github.com/sjep/array",
        sha_before="efa214159eaad2abda7b072f278d678f8788c307",
        sha_after="34380d886168061149af0ee47fe07a74b7ebf4e2",
    ),
    CVE(
        cve_id="CVE-2020-35898",
        crate_name="actix-utils",
        repo="https://github.com/actix/actix-net",
        sha_before="5d6d309e66b79d837ede20a26d654274bfb68d8f",
        sha_after="0dca1a705ad1ff4885b3491ecb809a808e1de66c",
    ),
    CVE(
        cve_id="CVE-2020-35901",
        crate_name="actix-http",
        repo="https://github.com/actix/actix-web",
        sha_before="3033f187d2214fdb40c815d982fbf6f8a31bcd3f",
        sha_after="fe13789345f6307e8ac1e1545770f82a14b6588b",
    ),
    CVE(
        cve_id="CVE-2020-35902",
        crate_name="actix-codec",
        repo="https://github.com/actix/actix-net",
        sha_before="693d5132a944b260b68071b938b6b2e532a7982c",
        sha_after="c41b5d8dd4235ccca84d0b687996615c0c64d956",
    ),
    CVE(
        cve_id="CVE-2020-35907",
        crate_name="futures-task",
        repo="https://github.com/rust-lang/futures-rs",
        sha_before="18bf8738dfd9f9f7fea6a308a45f763402dff8cb",
        sha_after="d98e6ecb231988ba0d017870ee95ca51a2c7315c",
    ),
    CVE(
        cve_id="CVE-2021-25905",
        crate_name="bra",
        repo="https://github.com/Enet4/bra-rs",
        sha_before="ff2b2995f43c24193aa71a3801bf7af287e2ca98",
        sha_after="aabf5562f8c6374ab30f615b28e0cff9b5c79e5f",
    ),
    CVE(
        cve_id="CVE-2021-45688",
        crate_name="ash",
        repo="https://github.com/ash-rs/ash",
        sha_before="17149bd791cd6e09b145063a238d3e27d855780c",
        sha_after="2c98b6f384a017de031698bd623551a45f24c8f9",
    ),
    CVE(
        cve_id="CVE-2022-23639",
        crate_name="crossbeam-utils",
        repo="https://github.com/crossbeam-rs/crossbeam",
        sha_before="be6ff29e1fce196519b65cb0b647f1ad72659498",
        sha_after="f7c378b26e273d237575154800f6c2bd3bf20058",
    ),
]
