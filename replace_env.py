#!/usr/bin/env python3

import os
import sys
import re

def replace_env_vars(file_path):
    matched_cnt = 0

    with open(file_path, 'r') as file:
        content = file.read()

    def replace_match(match):
        nonlocal matched_cnt
        var_name = match.group(1)
        matched_cnt += 1
        evar = os.environ.get(var_name, '')  # Return empty string if var not found
        if len(evar) == 0:
            raise ValueError(f"\"{var_name}\" unset")
        return evar

    pattern = re.compile(r'ENVREPLACE\[([A-Za-z_][A-Za-z0-9_]*)\]')
    new_content = pattern.sub(replace_match, content)

    with open(file_path, 'w') as file:
        file.write(new_content)

    return matched_cnt

if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("Usage: python3 replace_env_vars.py <file_path>")
        sys.exit(1)

    file_path = sys.argv[1]
    matched_cnt = replace_env_vars(file_path)
    print(f"Matched {matched_cnt} vars in {file_path}")
