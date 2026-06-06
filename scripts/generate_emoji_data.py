#!/usr/bin/env python3
"""Regenerate src/emoji_data.rs from twemoji-assets and Unicode emoji-test.txt."""

from __future__ import annotations

import json
import re
import subprocess
import urllib.request
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
OUTPUT = REPO_ROOT / "src" / "emoji_data.rs"
EMOJI_TEST_URL = "https://unicode.org/Public/emoji/latest/emoji-test.txt"
GROUP_ZH = {
    "Smileys & Emotion": "表情与情感",
    "People & Body": "人物与身体",
    "Component": "组件",
    "Animals & Nature": "动物与自然",
    "Food & Drink": "食物与饮料",
    "Travel & Places": "旅行与地点",
    "Activities": "活动",
    "Objects": "物品",
    "Symbols": "符号",
    "Flags": "旗帜",
}


def twemoji_codes_path() -> Path:
    result = subprocess.run(
        ["cargo", "metadata", "--format-version=1"],
        cwd=REPO_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    metadata = json.loads(result.stdout)
    for package in metadata["packages"]:
        if package["name"] == "twemoji-assets":
            path = Path(package["manifest_path"]).parent / "src" / "svg" / "codes.rs"
            if not path.exists():
                raise SystemExit(f"twemoji-assets codes.rs not found: {path}")
            return path
    raise SystemExit("twemoji-assets package not found in cargo metadata")


def load_twemoji_emojis(path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8")
    encoded = re.findall(r'svg_code!\([^,]+,\s*"((?:\\.|[^"])*)"\s*,', text)
    emojis = [json.loads(f'"{emoji}"') for emoji in encoded]
    if not emojis:
        raise SystemExit("no twemoji emojis parsed")
    if len(emojis) != len(set(emojis)):
        raise SystemExit("duplicate twemoji emoji literals parsed")
    return emojis


def fetch_emoji_test() -> str:
    with urllib.request.urlopen(EMOJI_TEST_URL, timeout=60) as response:
        return response.read().decode("utf-8")


def grouped_emojis(all_emojis: list[str], emoji_test: str) -> list[dict[str, object]]:
    groups: list[dict[str, object]] = []
    current: dict[str, object] | None = None
    all_set = set(all_emojis)
    seen: set[str] = set()
    line_re = re.compile(r"^[0-9A-F ]+\s*;\s*[^#]+#\s*(\S+)")

    for line in emoji_test.splitlines():
        if line.startswith("# group:"):
            source = line.split(":", 1)[1].strip()
            current = {"source": source, "name": GROUP_ZH.get(source, source), "emojis": []}
            groups.append(current)
            continue
        if current is None or line.startswith("#") or not line.strip():
            continue
        match = line_re.match(line)
        if not match:
            continue
        emoji = match.group(1)
        if emoji in all_set and emoji not in seen:
            current["emojis"].append(emoji)  # type: ignore[union-attr]
            seen.add(emoji)

    leftovers = [emoji for emoji in all_emojis if emoji not in seen]
    if leftovers:
        groups.append(
            {"source": "Uncategorized Twemoji Assets", "name": "未分组", "emojis": leftovers}
        )
    groups = [group for group in groups if group["emojis"]]
    grouped_count = sum(len(group["emojis"]) for group in groups)  # type: ignore[arg-type]
    if grouped_count != len(all_emojis):
        raise SystemExit(f"grouped {grouped_count}, expected {len(all_emojis)}")
    return groups


def rust_str(value: str) -> str:
    return json.dumps(value, ensure_ascii=False)


def write_output(all_emojis: list[str], groups: list[dict[str, object]]) -> None:
    lines = [
        "// Auto-generated from twemoji-assets svg/codes.rs + Unicode emoji-test.txt. Do not edit by hand.",
        "// Unicode emoji-test.txt provides CLDR group order; twemoji-assets provides the renderable SVG emoji set.",
        "",
        "pub struct EmojiGroup {",
        "    pub name: &'static str,",
        "    pub source_name: &'static str,",
        "    pub emojis: &'static [&'static str],",
        "}",
        "",
        "pub const ALL_TWEMOJI_EMOJIS: &[&str] = &[",
    ]
    lines.extend(f"    {rust_str(emoji)}," for emoji in all_emojis)
    lines.extend(["];", "", "pub const EMOJI_GROUPS: &[EmojiGroup] = &["])
    for group in groups:
        lines.extend(
            [
                "    EmojiGroup {",
                f"        name: {rust_str(str(group['name']))},",
                f"        source_name: {rust_str(str(group['source']))},",
                "        emojis: &[",
            ]
        )
        lines.extend(f"            {rust_str(emoji)}," for emoji in group["emojis"])  # type: ignore[index]
        lines.extend(["        ],", "    },"])
    lines.extend(["];", ""])
    OUTPUT.write_text("\n".join(lines), encoding="utf-8")


def main() -> None:
    all_emojis = load_twemoji_emojis(twemoji_codes_path())
    groups = grouped_emojis(all_emojis, fetch_emoji_test())
    write_output(all_emojis, groups)
    print(f"Generated {len(all_emojis)} emoji across {len(groups)} groups -> {OUTPUT}")


if __name__ == "__main__":
    main()
