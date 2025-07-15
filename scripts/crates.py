import dataclasses
import re
import sys
import toml

@dataclasses.dataclass
class Info:
    name: str
    files: int
    code: int
    comment: int
    blank: int
    total: int

if __name__ == "__main__":
    input = sys.argv[1] if len(sys.argv) > 1 else "/home/rpl/.VSCodeCounter/2025-07-15_11-27-53/results.md"
    crates_input = sys.argv[2] if len(sys.argv) > 2 else "/home/rpl/RPL-Pest/crates/lintcheck/1000.toml"
    info_list: list[Info] = []
    crates = toml.load(crates_input)
    crates_dict: dict[str, str] = {}
    for crate_name, crate_data in crates["crates"].items():
        crates_dict[crate_name + '-' + crate_data["version"]] = crate_name
    with open(input) as f:
        # | path | files | code | comment | blank | total |
        regex = re.compile('\| ([^/\(\)]+) \| ([0-9,]+) \| ([0-9,]+) \| ([0-9,]+) \| ([0-9,]+) \| ([0-9,]+) \|')
        for l in f.readlines():
            if regex.match(l):
                m = regex.match(l)
                info = Info(
                    m.group(1),
                    int(m.group(2).replace(',', '')),
                    int(m.group(3).replace(',', '')),
                    int(m.group(4).replace(',', '')),
                    int(m.group(5).replace(',', '')),
                    int(m.group(6).replace(',', '')),
                )
                # print(l)
                if info.name in crates_dict:
                    info_list.append(info)
                else:
                    print(f"Unknown crate: {info.name}")
        info_list.sort(key=lambda x: x.code)
        print(f"Got {len(info_list)} items")
        # for info in info_list[-50:]:
        #     print(info)
        selected = [crates_dict[info.name] for info in info_list if info.name in crates_dict][19::20]
        print(len(selected))
        selected_dict = {
            "crates": { name: crates["crates"][name] for name in selected }
        }
        print(toml.dumps(selected_dict))
