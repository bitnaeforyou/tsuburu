#!/usr/bin/env python3
"""한국어 -> 영어 검색어 사전을 만든다.

출처: 퍼블릭 도메인으로 공개된 태그 대응표. 주소는 인자로 준다.
출력: crates/tsuburu-korean/data/korean.tsv

hitomi의 갤러리 B-tree 인덱스는 네임스페이스가 붙은 형태를 받지 않는다.
`female:glasses`로는 0건, `glasses`로는 정상 검색된다(실측). 그래서 양쪽
모두에서 접두사를 떼어낸다.

재생성:
    python3 tools/build-korean-dictionary.py <태그표의 기준 URL>
"""
import json
import os
import re
import sys
import urllib.request
from pathlib import Path

BASE = os.environ.get("TAG_INFO_BASE") or sys.argv[1]

# 앞에 오는 것이 후보 순서에서 우선한다. 태그가 검색에 가장 쓸모 있다.
SOURCES = [
    ("result-korean-tag.json", 0),
    ("result-korean-series.json", 1),
    ("result-korean-merge.json", 2),
    ("result-korean-character.json", 3),
]

HANGUL = re.compile(r"[가-힣]")
NAMESPACE = re.compile(r"^(?:female|male|tag|character|series|artist|group|language|type):")


def strip_namespace(text: str) -> str:
    # `tag:female:big breasts`처럼 접두사가 겹쳐 있는 경우가 있어 반복해서 벗긴다.
    previous = None
    while previous != text:
        previous = text
        text = NAMESPACE.sub("", text).strip()
    return text


def fetch(name: str) -> dict:
    with urllib.request.urlopen(f"{BASE}/{name}", timeout=60) as response:
        return json.loads(response.read())


def main() -> int:
    out_path = Path(__file__).resolve().parent.parent / "crates/tsuburu-korean/data/korean.tsv"

    # 한국어 -> {영어: 우선순위}
    entries: dict[str, dict[str, int]] = {}

    for name, priority in SOURCES:
        try:
            data = fetch(name)
        except Exception as err:  # noqa: BLE001
            print(f"  skipped {name}: {err}", file=sys.stderr)
            continue

        kept = 0
        for english, korean in data.items():
            if not isinstance(korean, str) or not HANGUL.search(korean):
                continue
            ko = strip_namespace(korean).lower()
            en = strip_namespace(english).lower()
            if not ko or not en or ko == en:
                continue
            candidates = entries.setdefault(ko, {})
            if en not in candidates or priority < candidates[en]:
                candidates[en] = priority
            kept += 1
        print(f"  {name}: {kept} usable of {len(data)}", file=sys.stderr)

    lines = []
    for korean in sorted(entries):
        # 우선순위, 그다음 사전순. 순서가 고정이어야 같은 입력이 같은 결과를 낸다.
        english = sorted(entries[korean], key=lambda e: (entries[korean][e], e))
        lines.append(korean + "\t" + "\t".join(english))

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {len(lines)} entries to {out_path}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
