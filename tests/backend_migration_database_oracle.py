"""Original SQLite transaction moves all durable key references and signed hashes."""
from contextlib import closing
import json
from pathlib import Path
import sqlite3
from tests.backend_source_oracle import source_namespace, validate_cases
SOURCE = "d173116:migration_bundle.rewrite_database"
FIXTURE = Path(__file__).parent / "fixtures/backend/migration-database.json"


def fixtures():
    source = source_namespace("migration_bundle.py", ("rewrite_database",), {
        "REFERENCES": (("items", "content_key"), ("publications", "content_key"), ("seen_files", "content_key"))})
    cases = [
        {"name": "swap-all-references", "map": {"v:a": "v:b", "v:b": "v:a"}, "hashes": {"v:a": 1 << 63, "v:b": 0}},
        {"name": "move-and-null-hash", "map": {"v:a": "v:c"}, "hashes": {"v:c": None}},
    ]
    for case in cases:
        with closing(sqlite3.connect(":memory:")) as db:
            db.executescript("CREATE TABLE items(content_key TEXT PRIMARY KEY,phash INTEGER,label TEXT);"
                             "CREATE TABLE publications(content_key TEXT,custom_emoji_id TEXT);"
                             "CREATE TABLE seen_files(content_key TEXT,file_unique_id TEXT);"
                             "INSERT INTO items VALUES('v:a',123,'alpha'),('v:b',456,'beta');"
                             "INSERT INTO publications VALUES('v:a','111111111'),('v:b','222222222');"
                             "INSERT INTO seen_files VALUES('v:a','unique-a'),('v:b','unique-b');")
            signed = {key: None if value is None else value if value < 1 << 63 else value - (1 << 64)
                      for key, value in case["hashes"].items()}
            moved = source["rewrite_database"](db, case["map"], signed)
            case["expected"] = {"moved": moved, "items": db.execute("SELECT * FROM items ORDER BY content_key").fetchall(),
                                "publications": db.execute("SELECT * FROM publications ORDER BY content_key").fetchall(),
                                "seen_files": db.execute("SELECT * FROM seen_files ORDER BY content_key").fetchall()}
    value = {"source": SOURCE, "cases": cases}
    # Fixture JSON is the public interchange format, so SQLite tuples become arrays.
    value = json.loads(json.dumps(value))
    validate_cases(value, source=SOURCE, count=2)
    return value


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    value = fixtures()
    if args.write:
        FIXTURE.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    elif json.loads(FIXTURE.read_text(encoding="utf-8")) != value:
        raise SystemExit("Fixture drift: migration-database.json")
