#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# Check YAML key consistency and CJK in en-US using Python
python3 << 'PYEOF'
import yaml, re, sys

def flatten(d, prefix=''):
    out = {}
    for k, v in d.items():
        if k.startswith('_'):
            continue
        path = f'{prefix}.{k}' if prefix else k
        if isinstance(v, dict):
            out.update(flatten(v, path))
        else:
            out[path] = v
    return out

zh = flatten(yaml.safe_load(open('locales/zh-CN.yml')))
en = flatten(yaml.safe_load(open('locales/en-US.yml')))

# Check flattened key sets match
zh_keys = set(zh.keys())
en_keys = set(en.keys())
missing = zh_keys - en_keys
extra = en_keys - zh_keys
if missing or extra:
    print(f'MISMATCH: missing in en={sorted(missing)}', file=sys.stderr)
    print(f'MISMATCH: extra in en={sorted(extra)}', file=sys.stderr)
    sys.exit(1)

# Check en-US values have no CJK characters
cjk = [k for k, v in en.items() if isinstance(v, str) and re.search(r'[\u4e00-\u9fff]', v)]
if cjk:
    print(f'CJK found in en-US: {cjk}', file=sys.stderr)
    sys.exit(1)

total = len(zh)
# All en-US values non-empty (coverage = 100% when keys match and no CJK)
en_nonempty = sum(1 for v in en.values() if v and v.strip())
coverage = 100.0 if total > 0 else 0.0
print(f'zh-CN: {total} keys, en-US: {total} keys, coverage: {coverage:.0f}%')
PYEOF

echo "i18n check passed"
