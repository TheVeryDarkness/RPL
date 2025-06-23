clear

set -eux

function uitest() {
    RPL_PATS="$1" cargo uitest -- "$2"
}

uitest "docs/patterns-pest/CVE-2018-20992.rpl"                                                                  "tests/ui/cve_2018_20992" # Duplicates
uitest "docs/patterns-pest/CVE-2018-21000.rpl"                                                                  "tests/ui/cve_2018_21000"
# uitest "docs/patterns-pest/CVE-2019-15548.rpl"                                                                  "tests/ui/cve_2019_15548" # Order changed
# uitest "docs/patterns-pest/CVE-2019-16138.rpl:docs/patterns-pest/private-or-generic-function-marked-inline.rpl" "tests/ui/cve_2019_16138" # Order changed
# uitest "docs/patterns-pest/CVE-2020-25016.rpl:docs/patterns-pest/private-or-generic-function-marked-inline.rpl" "tests/ui/cve_2020_25016" # Order changed
uitest "docs/patterns-pest/CVE-2020-35860.rpl:docs/patterns-pest/private-or-generic-function-marked-inline.rpl" "tests/ui/cve_2020_35860"
uitest "docs/patterns-pest/CVE-2020-35877.rpl"                                                                  "tests/ui/cve_2020_35877"
uitest "docs/patterns-pest/CVE-2020-35888.rpl"                                                                  "tests/ui/cve_2020_35888" # Duplicates
# uitest "docs/patterns-pest/CVE-2020-35898-9.rpl"                                                                "tests/ui/cve_2020_35898_9" # Unreasonable diagnostic messages, fixable but I'm not sure if it's worth it
uitest "docs/patterns-pest/CVE-2020-35901-2.rpl"                                                                "tests/ui/cve_2020_35901_2"
uitest "docs/patterns-pest/CVE-2020-35907.rpl"                                                                  "tests/ui/cve_2020_35907"
uitest "docs/patterns-pest/CVE-2021-25904.rpl"                                                                  "tests/ui/cve_2021_25904"
uitest "docs/patterns-pest/CVE-2021-25905.rpl:docs/patterns-pest/CVE-2021-29941-2.rpl"                          "tests/ui/cve_2021_25905"
# uitest "docs/patterns-pest/CVE-2021-27376.rpl"                                                                  "tests/ui/cve_2021_27376" # Order changed
# uitest "docs/patterns-pest/CVE-2021-29941-2.rpl:docs/patterns-pest/CVE-2019-16138.rpl"                          "tests/ui/cve_2021_29941_2" # Order changed
uitest "docs/patterns-pest/CVE-2024-27284.rpl"                                                                  "tests/ui/cve_2024_27284"
uitest "docs/patterns-pest/manually-drop.rpl"                                                                   "tests/ui/std/mem/ManuallyDrop"
uitest "docs/patterns-pest/cast-size-different-sizes.rpl"                                                       "tests/ui/cast_size_different_sizes"
uitest "docs/patterns-pest/unsound-collection-transmute.rpl"                                                    "tests/ui/unsound_collection_transmute"
