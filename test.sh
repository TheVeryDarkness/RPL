#!/bin/sh

clear

set -eux

# export RPL_LOG=trace
export RUSTC_COLOR=always
export RUSTC_LOG_COLOR=always

# export RPL_PATS=docs/patterns-pest/cve/CVE-2020-35897.rpl
cargo +rpl-dbg run --bin rpl-driver --color always -- --color always tests/ui/cve/cve_2020_35897/minimal.rs -Z no-codegen 2>&1 | tee .ansi

# export RPL_PATS=docs/patterns-pest/cve/CVE-2020-35905.rpl
cargo +rpl-dbg run --bin rpl-driver --color always -- --color always tests/ui/cve/cve_2020_35905/minimal.rs -Z no-codegen 2>&1 | tee .ansi

# export RPL_PATS=docs/patterns-pest/cve/CVE-2020-35886.rpl
cargo +rpl-dbg run --bin rpl-driver --color always -- --color always tests/ui/cve/cve_2020_35886/minimal.rs -Z no-codegen 2>&1 | tee .ansi
