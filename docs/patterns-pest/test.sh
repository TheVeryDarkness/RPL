clear

set -eux

# export RPL_PATS="docs/patterns-pest/CVE-2018-20992.rpl"
# cargo uitest -- "tests/ui/cve_2018_20992" Duplicates

export RPL_PATS="docs/patterns-pest/CVE-2018-21000.rpl"
cargo uitest -- "tests/ui/cve_2018_21000"

# export RPL_PATS="docs/patterns-pest/CVE-2019-15548.rpl"
# cargo uitest -- "tests/ui/cve_2019_15548" # Order changed

# export export RPL_PATS="docs/patterns-pest/CVE-2019-16138.rpl:docs/patterns-pest/private-or-generic-function-marked-inline.rpl"
# cargo uitest -- "tests/ui/cve_2019_16138" # Order changed

# export RPL_PATS="docs/patterns-pest/CVE-2020-25016.rpl:docs/patterns-pest/private-or-generic-function-marked-inline.rpl"
# cargo uitest -- "tests/ui/cve_2020_25016" # Order changed

export RPL_PATS="docs/patterns-pest/CVE-2020-35860.rpl:docs/patterns-pest/private-or-generic-function-marked-inline.rpl"
cargo uitest -- "tests/ui/cve_2020_35860"

RPL_PATS="docs/patterns-pest/CVE-2020-35877.rpl"               cargo uitest -- "tests/ui/cve_2020_35877"
RPL_PATS="docs/patterns-pest/CVE-2020-35888.rpl"               cargo uitest -- "tests/ui/cve_2020_35888"
RPL_PATS="docs/patterns-pest/CVE-2020-35898-9.rpl"             cargo uitest -- "tests/ui/cve_2020_35898_9"
RPL_PATS="docs/patterns-pest/CVE-2020-35901-2.rpl"             cargo uitest -- "tests/ui/cve_2020_35901_2"
RPL_PATS="docs/patterns-pest/CVE-2020-35907.rpl"               cargo uitest -- "tests/ui/cve_2020_35907"
RPL_PATS="docs/patterns-pest/CVE-2021-25904.rpl"               cargo uitest -- "tests/ui/cve_2021_25904"
RPL_PATS="docs/patterns-pest/CVE-2021-25905.rpl"               cargo uitest -- "tests/ui/cve_2021_25905"
RPL_PATS="docs/patterns-pest/CVE-2021-27376.rpl"               cargo uitest -- "tests/ui/cve_2021_27376"
RPL_PATS="docs/patterns-pest/CVE-2021-29941-2.rpl"             cargo uitest -- "tests/ui/cve_2021_29941_2"
RPL_PATS="docs/patterns-pest/CVE-2024-27284.rpl"               cargo uitest -- "tests/ui/cve_2024_27284"
RPL_PATS="docs/patterns-pest/manually-drop.rpl"                cargo uitest -- "tests/ui/std/mem/ManuallyDrop"
RPL_PATS="docs/patterns-pest/cast-size-different-sizes.rpl"    cargo uitest -- "tests/ui/cast_size_different_sizes"
RPL_PATS="docs/patterns-pest/unsound-collection-transmute.rpl" cargo uitest -- "tests/ui/unsound_collection_transmute"
