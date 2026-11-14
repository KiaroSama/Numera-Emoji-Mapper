//! Original fake-client guard uses literal argument shapes, never imported client/test modules.
use _native::{source_fake_contracts as policy, source_git_inventory::Source};
use sha2::{Digest, Sha256};

fn source(path: &str, text: &str) -> Source {
    Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: Sha256::digest(text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        text: text.into(),
    }
}
fn inspect(fake: &str) -> policy::Report {
    let real = "raise RuntimeError('never execute')\nclass Telegram:\n    def _call(self, method, *, data=None, retries=5): pass\n    call = _call\n    def _safe(self, exc): pass\n    safe = _safe\n    def send_message(self, chat, text, *, disable_preview=False): pass\n    def download_file(self, file_id, dest, retries=5): pass\n    def positional(self, one, /, optional=1): pass\n";
    policy::inspect(&[
        source("emojikit/telegram_api.py", real),
        source("tests/_fakes.py", fake),
    ])
    .unwrap()
}

#[test]
fn original_keyword_and_positional_contracts_preserve_defaults_aliases_and_variadic_exemptions() {
    let good = "class Fake:\n    def send_message(self, chat, text, disable_preview=False): pass\n    def download_file(self, file_id, dest): pass\n    def _call(self, method, **options): pass\n    def call(self, method, *, data=None, retries=5): pass\n    def safe(self, exc): pass\n    def positional(self, one, /): pass\n";
    let report = inspect(good);
    assert_eq!(report.real_methods, 7);
    assert_eq!(report.fake_methods, 6);
    policy::check(&report).unwrap();
    for (fake, keyword, positional) in [
        (
            "class Fake:\n    def send_message(self, chat, text): pass\n",
            1,
            0,
        ),
        (
            "class Fake:\n    def send_message(self, chat, **options): pass\n",
            0,
            1,
        ),
        (
            "class Fake:\n    def send_message(self, *values): pass\n",
            1,
            0,
        ),
        (
            "class Fake:\n    def send_message(self, *values, **options): pass\n",
            0,
            0,
        ),
        (
            "class Fake:\n    def download_file(self, file_id): pass\n",
            0,
            1,
        ),
        (
            "class Fake:\n    def positional(self, *, one): pass\n",
            0,
            1,
        ),
        ("class Fake:\n    def call(self, method): pass\n", 2, 0),
        ("class Fake:\n    def safe(self): pass\n", 0, 1),
    ] {
        let report = inspect(fake);
        assert_eq!(report.keyword_errors.len(), keyword, "{fake}");
        assert_eq!(report.positional_errors.len(), positional, "{fake}");
    }
    let ignored = inspect(
        "# class Fake: def send_message(self): pass\nx = 'send_message'\nclass Case(unittest.TestCase):\n    def send_message(self): pass\nclass Fake:\n    async def send_message(self): pass\n",
    );
    assert_eq!(ignored.fake_methods, 0);
    assert!(policy::check(&ignored).is_err());
}

#[test]
fn source_admission_and_dynamic_real_surfaces_fail_instead_of_certifying_empty_or_unknown_scans() {
    for real in [
        "class Telegram:\n    @decorator\n    def call(self): pass\n",
        "class Telegram(Base):\n    def call(self): pass\n",
        "class Telegram:\n    call = factory()\n",
        "class Telegram:\n    def call(self): pass\n    if flag: other = call\n",
        "class Telegram:\n    def call(self): pass\n    del call\n",
        "class Telegram:\n    def call(self): pass\n    call = missing\n",
        "def broken(",
        "pass\n",
    ] {
        assert!(
            policy::inspect(&[source("emojikit/telegram_api.py", real)]).is_err(),
            "{real}"
        );
    }
    let real = source(
        "emojikit/telegram_api.py",
        "class Telegram:\n    def call(self): pass\n",
    );
    let mut fake = source("tests/_fakes.py", "class Fake:\n    def call(self): pass\n");
    fake.text.push_str("raise RuntimeError('hash drift')\n");
    assert!(policy::inspect(&[source(&real.path, &real.text), fake]).is_err());
    assert!(
        policy::inspect(&[
            source(&real.path, &real.text),
            source(&real.path, &real.text)
        ])
        .is_err()
    );
    assert!(policy::inspect(&[source("../bad.py", "pass")]).is_err());
    assert!(
        policy::inspect(&[source(
            "emojikit/telegram_api.py",
            &" ".repeat(2 * 1024 * 1024 + 1)
        )])
        .is_err()
    );
    assert!(policy::check(&policy::Report::default()).is_err());
}
