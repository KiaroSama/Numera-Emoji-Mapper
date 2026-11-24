//! The sole reviewed motion-gate delta from the independent frozen gallery HTML.

pub fn with_reviewed_motion_gate(expected: &str) -> String {
    // Preserve the frozen fixture and every unrelated byte. Both full-page
    // parity consumers use these literal transitions, never current assets.
    let mut expected = expected.to_owned();
    for (before, after) in [
        (
            "function setLive(n, live) {\n  if (!n) return;\n",
            "function mayAnimate() {\n  return ANIM_ON && !RM && !document.hidden && thaw === null;\n}\nfunction setLive(n, live) {\n  if (!n) return;\n  live = live && mayAnimate();\n",
        ),
        (
            "  if (!ANIM_ON || RM) { freezeAll(); return; }\n",
            "  if (!mayAnimate()) { freezeAll(); return; }\n",
        ),
    ] {
        assert_eq!(
            expected.matches(before).count(),
            1,
            "frozen gallery motion transition must match exactly once"
        );
        expected = expected.replacen(before, after, 1);
    }
    expected
}
