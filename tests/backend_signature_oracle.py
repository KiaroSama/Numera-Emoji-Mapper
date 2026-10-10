"""Pinned version2 SQL signature, exact repr order and CPython3.11 Unicode14 printability."""
from contextlib import closing
import hashlib
import json
from pathlib import Path
import sqlite3
import sys
import unicodedata
from tests.backend_source_oracle import source_namespace, validate_cases
ROOT = Path(__file__).resolve().parent.parent
FIXTURE = ROOT / "tests/fixtures/backend/signature.json"
TABLE = ROOT / "native/data/python311-nonprintable.json"
SOURCE = "d173116:migration_bundle.database_signature"


def unicode_table():
    if sys.version_info[:2] != (3, 11) or unicodedata.unidata_version != "14.0.0":
        raise ValueError("Unicode table regeneration requires CPython3.11/UCD14.0.0")
    ranges = []
    start = None
    for codepoint in range(0x110000):
        if not chr(codepoint).isprintable():
            if start is None:
                start = codepoint
        elif start is not None:
            ranges.append([start, codepoint - 1])
            start = None
    if start is not None:
        ranges.append([start, 0x10ffff])
    return {"python": "3.11", "unicode": "14.0.0", "nonprintable": ranges}


def fixtures():
    source = source_namespace("migration_bundle.py", ("_quote", "database_signature"), {"hashlib": hashlib, "json": json})
    rows = [["quote'only", -9223372036854775808, None, {"blob": "00090a0d1f207f80ff2722"}],
            ['both\'"quotes', 9223372036854775807, "دو 😀​ \u007f ", {"blob": "68656c6c6f"}],
            ["back\\slash\t\n\r\b\f", 0, "<>&", {"blob": ""}],
            ["ordinary", -1, "./after.png", {"blob": "2727"}],
            ["ordinary", -1, "./after.png", {"blob": "2727"}]]
    cases = []
    for normalize in (False, True):
        with closing(sqlite3.connect(":memory:")) as db:
            db.executescript('CREATE TABLE "z weird"(text TEXT,n INTEGER,nullable TEXT,b BLOB);'
                             'CREATE TABLE items(content_key TEXT,file_path TEXT,phash INTEGER);'
                             'CREATE TABLE aa(note TEXT);INSERT INTO aa VALUES("first table");')
            values = [[bytes.fromhex(v["blob"]) if isinstance(v, dict) else v for v in row] for row in rows]
            db.executemany('INSERT INTO "z weird" VALUES(?,?,?,?)', values)
            db.execute("INSERT INTO items VALUES(?,?,?)", ("s:fixture", "./after.png", -1))
            files = [{"source": "./before.png", "destination": "./after.png"}] if normalize else []
            cases.append({"name": "normalized" if normalize else "raw", "rows": rows, "files": files,
                          "expected": source["database_signature"](db, files), "repr_order": sorted([repr(v) for v in values])})
    value = {"source": SOURCE, "cases": cases}
    validate_cases(value, source=SOURCE, count=2)
    return value


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    parser.add_argument("--unicode-table", action="store_true")
    args = parser.parse_args()
    value = fixtures()
    if args.write:
        FIXTURE.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    elif json.loads(FIXTURE.read_text(encoding="utf-8")) != value:
        raise SystemExit("Fixture drift: signature.json")
    if args.unicode_table:
        TABLE.parent.mkdir(exist_ok=True)
        TABLE.write_text(json.dumps(unicode_table(), separators=(",", ":")) + "\n", encoding="utf-8")
