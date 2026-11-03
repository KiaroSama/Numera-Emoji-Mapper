//! Original repaint warnings and explicit consent, without a terminal, child or network.
use _native::repaint_gate::{self, MODES};
use serde_json::json;
use std::cell::Cell;
use std::io;

fn labels() -> Vec<String> {
    vec!["a".into(), "b".into()]
}

#[test]
fn per_sticker_flags_never_inherit_the_set_level_answer() {
    for (sticker, expected) in [
        (json!({"needs_repainting":true}), true),
        (json!({"emoji":"😀"}), false),
        (json!({"needs_repainting":false}), false),
        (json!({}), false),
        (
            json!({"needs_repainting":null,"stickers":vec![json!({"needs_repainting":true});3]}),
            false,
        ),
    ] {
        assert_eq!(repaint_gate::is_repaintable(&sticker), expected);
    }
    let pack = json!({"needs_repainting":null,"stickers":vec![json!({"needs_repainting":true});3]});
    assert!(!repaint_gate::is_repaintable(&pack));
    assert!(
        pack["stickers"]
            .as_array()
            .unwrap()
            .iter()
            .all(repaint_gate::is_repaintable)
    );
    // Original bool(get(...)) semantics are distinct from reading a set's flag.
    for value in [json!(1), json!("yes"), json!([false]), json!({"a":false})] {
        assert!(repaint_gate::is_repaintable(
            &json!({"needs_repainting":value})
        ));
    }
}

#[test]
fn original_modes_explicit_answers_and_no_answer_guidance_are_preserved() {
    assert_eq!(MODES, ["ask", "skip", "keep"]);
    let calls = Cell::new(0);
    let mut output = Vec::new();
    assert!(
        repaint_gate::decide(
            &[],
            "ask",
            true,
            |_| {
                calls.set(calls.get() + 1);
                Ok(Some("n".into()))
            },
            &mut output
        )
        .unwrap()
    );
    assert!(output.is_empty());
    assert_eq!(calls.get(), 0);
    for (mode, expected) in [("skip", false), ("keep", true)] {
        output.clear();
        assert_eq!(
            repaint_gate::decide(
                &labels(),
                mode,
                true,
                |_| { panic!("explicit mode must never prompt") },
                &mut output
            )
            .unwrap(),
            expected
        );
        let text = String::from_utf8(output.clone()).unwrap();
        assert!(text.contains("2 of these emoji are REPAINTABLE: a, b"));
        assert!(text.contains(&format!("--repaintable {mode}")));
    }
    for (answer, expected) in [
        ("y", true),
        ("YES", true),
        (" \u{1c}Yes\u{1f} ", true),
        ("n", false),
        ("", false),
        ("no", false),
        ("maybe", false),
        ("  N  ", false),
    ] {
        output.clear();
        assert_eq!(
            repaint_gate::decide(
                &labels(),
                "ask",
                true,
                |prompt| {
                    assert_eq!(prompt, "  Ingest them anyway? [y/N] ");
                    Ok(Some(answer.into()))
                },
                &mut output
            )
            .unwrap(),
            expected,
            "{answer:?}"
        );
        assert!(
            String::from_utf8(output.clone())
                .unwrap()
                .contains("REPAINTABLE")
        );
    }
    output.clear();
    assert!(
        !repaint_gate::decide(
            &labels(),
            "ask",
            false,
            |_| panic!("no terminal"),
            &mut output
        )
        .unwrap()
    );
    let text = String::from_utf8(output.clone()).unwrap();
    assert!(text.contains("nobody answered"));
    assert!(text.contains("--repaintable keep"));
    for error in [
        None,
        Some(io::ErrorKind::UnexpectedEof),
        Some(io::ErrorKind::Interrupted),
    ] {
        output.clear();
        assert!(
            !repaint_gate::decide(
                &labels(),
                "ask",
                true,
                |_| {
                    match error {
                        Some(kind) => Err(io::Error::from(kind)),
                        None => Ok(None),
                    }
                },
                &mut output
            )
            .unwrap()
        );
        assert!(
            String::from_utf8(output.clone())
                .unwrap()
                .contains("nobody answered")
        );
    }
    assert!(
        repaint_gate::decide(
            &labels(),
            "invalid",
            false,
            |_| panic!("invalid mode"),
            &mut output
        )
        .is_err()
    );
    assert!(
        repaint_gate::decide(
            &labels(),
            "ask",
            true,
            |_| Err(io::Error::from(io::ErrorKind::PermissionDenied)),
            &mut output
        )
        .is_err()
    );
}

#[test]
fn warning_bounds_labels_and_output_errors_cannot_turn_into_consent() {
    let labels = (0..30).map(|i| format!("id{i}")).collect::<Vec<_>>();
    let mut output = Vec::new();
    assert!(!repaint_gate::decide(&labels, "skip", true, |_| panic!("skip"), &mut output).unwrap());
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("30 of these emoji are REPAINTABLE: id0"));
    assert!(text.contains("id7, ... (+22 more)"));
    assert!(!text.contains("id8"));
    struct Refused;
    impl io::Write for Refused {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    assert!(
        repaint_gate::decide(
            &labels,
            "keep",
            true,
            |_| panic!("write failed"),
            &mut Refused
        )
        .is_err()
    );
}
