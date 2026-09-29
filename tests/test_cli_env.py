"""emojikit.cli_env: reading .env the way every entry point does."""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path


from emojikit import cli_env


class ADotEnvWithAByteOrderMark(unittest.TestCase):
    """Windows PowerShell 5.1 saves UTF-8 with a BOM.

    Read as plain utf-8 the first key became "\\ufeffKEY", so that one setting
    read as unset -- while put-secrets.ps1, which strips the mark, saw it.
    """

    def test_the_first_key_is_read_under_its_own_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            env = Path(tmp) / ".env"
            env.write_bytes("﻿FIRST_KEY=one\n# note\nSECOND='two'\n"
                            .encode("utf-8"))
            self.assertEqual(cli_env._parse_env_file(env),
                             {"FIRST_KEY": "one", "SECOND": "two"})


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_cli_env -v")
