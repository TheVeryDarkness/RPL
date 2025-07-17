import dataclasses
import re
import os
import sys
import toml
import glob
import subprocess
import shutil

@dataclasses.dataclass
class Info:
    name: str
    time: list[float]

def main():
    crates_input = sys.argv[1] if len(sys.argv) > 1 else "/home/rpl/RPL-Pest/crates/lintcheck/1000.toml"
    crates_output = sys.argv[2] if len(sys.argv) > 2 else "/home/rpl/RPL-Pest/crates/lintcheck/1000.csv"
    target_dir = sys.argv[3] if len(sys.argv) > 3 else "/home/rpl/target/"
    sources_dir = sys.argv[4] if len(sys.argv) > 4 else "/home/rpl/RPL-Pest/target/lintcheck/sources/"
    info_list: list[Info] = []
    crates = toml.load(crates_input)
    crates_dict: dict[str, str] = {}
    for crate_name, crate_data in crates["crates"].items():
        crates_dict[crate_name + '-' + crate_data["version"]] = crate_name
    for crate_name, crate_data in crates["crates"].items():
        print(f"Processing crate: {crate_name} ({crate_data['version']})")
        crate_dir = f"{sources_dir}/{crate_name}-{crate_data["version"]}/"
        lints = [
            "cast_slice_different_sizes",
            "eager_transmute",
            "mem_replace_with_uninit",
            "mut_from_ref",
            "not_unsafe_ptr_arg_deref",
            "size_of_in_element_count",
            "transmute_null_to_fn",
            "transmuting_null",
            "uninit_assumed_init",
            "uninit_vec",
            "unsound_collection_transmute",
            "zst_offset",
        ]
        info = Info(crate_name, [])
        for _ in range(5):
            if os.path.exists(target_dir):
                shutil.rmtree(target_dir)
            child = subprocess.run(["cargo", "+nightly", "clippy", "--target-dir", target_dir, "--", "-Z", "time-passes", *['-Wclippy::'+lint for lint in lints]], stderr=subprocess.PIPE, encoding='utf-8', cwd=crate_dir)
            if child.returncode != 0:
                # print(child.stderr)
                continue
            # child.check_returncode()
            regex = r"time: *([0-9.]+); rss: *.+ -> *.+ \( *.+\)\W*([^ ]+)"
            total_time = 0.0
            for l in child.stderr.splitlines():
                m = re.search(regex, l)
                if not m:
                    # print(f"Failed to parse line: {l}")
                    continue
                if m.group(2) == "lint_checking":
                    time = float(m.group(1))
                    total_time += time
                # print(m.groups())
            info.time.append(total_time)
        info_list.append(info)
        print(info)
    for info in info_list:
        print(f"{info.name},{info.time}")
    index = 0
    with open(crates_output, "w") as f:
        f.write("序号,包名,时间\n")
        for info in info_list:
            index += 1
            f.write(f"{index},{info.name},{info.time}\n")


if __name__ == "__main__":
    main()
