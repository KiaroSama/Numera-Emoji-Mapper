use super::*;
mod the_whole_timeline_is_compared {
    use super::*;
    // tests.test_video_timeline_comparison.TheWholeTimelineIsCompared.test_a_re_encode_of_the_same_clip_is_still_the_same_clip
    #[test]
    fn test_a_re_encode_of_the_same_clip_is_still_the_same_clip() {
        let f = Timelines::new(&["blue", "blue_again"]);
        assert_eq!(
            same(&f.scratch.0, f.get("blue"), f.get("blue_again")),
            Some(true)
        );
    }
    // tests.test_video_timeline_comparison.TheWholeTimelineIsCompared.test_the_same_opening_with_a_different_tail_is_not
    #[test]
    fn test_the_same_opening_with_a_different_tail_is_not() {
        let f = Timelines::new(&["blue", "green"]);
        assert_eq!(
            same(&f.scratch.0, f.get("blue"), f.get("green")),
            Some(false)
        );
    }
    // tests.test_video_timeline_comparison.TheWholeTimelineIsCompared.test_a_shorter_clip_is_not_the_same_clip
    #[test]
    fn test_a_shorter_clip_is_not_the_same_clip() {
        let f = Timelines::new(&["blue", "short"]);
        assert_eq!(
            same(&f.scratch.0, f.get("blue"), f.get("short")),
            Some(false)
        );
    }
    // tests.test_video_timeline_comparison.TheWholeTimelineIsCompared.test_a_fade_in_is_not_the_solid_version_of_itself
    #[test]
    fn test_a_fade_in_is_not_the_solid_version_of_itself() {
        let f = Timelines::new(&["fade", "solid"]);
        assert_eq!(
            same(&f.scratch.0, f.get("fade"), f.get("solid")),
            Some(false)
        );
    }
    // tests.test_video_timeline_comparison.TheWholeTimelineIsCompared.test_one_differing_frame_in_the_middle_is_not
    #[test]
    fn test_one_differing_frame_in_the_middle_is_not() {
        let f = Timelines::new(&["blue", "spiked"]);
        assert_eq!(
            key(&f.scratch.0, f.get("blue")),
            key(&f.scratch.0, f.get("spiked"))
        );
        assert_eq!(
            same(&f.scratch.0, f.get("blue"), f.get("spiked")),
            Some(false)
        );
    }
    // tests.test_video_timeline_comparison.TheWholeTimelineIsCompared.test_an_unreadable_clip_is_undecidable_not_negative
    #[test]
    fn test_an_unreadable_clip_is_undecidable_not_negative() {
        let f = Timelines::new(&["blue"]);
        let bad = file(
            &f.scratch.join("broken.webm"),
            b"\x1a\x45\xdf\xa3not a real stream",
        );
        assert_eq!(same(&f.scratch.0, f.get("blue"), &bad), None);
    }
    // tests.test_video_timeline_comparison.TheWholeTimelineIsCompared.test_the_comparison_is_bounded
    #[test]
    fn test_the_comparison_is_bounded() {
        let f = Timelines::new(&["blue"]);
        let timeline = runtime()
            .block_on(decoder(&f.scratch.0).timeline_typed(f.get("blue")))
            .unwrap();
        assert!(timeline.times.len() <= 128);
        assert_eq!(
            timeline.rgba.len(),
            timeline.times.len() * rt::codec_video_timeline::FRAME_BYTES
        );
    }
}
mod reconciliation_writes_nothing_for_a_foreign_clip {
    use super::*;
    // tests.test_video_timeline_comparison.ReconciliationWritesNothingForAForeignClip.test_a_foreign_clip_sharing_our_opening_resolves_to_nothing
    #[test]
    fn test_a_foreign_clip_sharing_our_opening_resolves_to_nothing() {
        let f = Timelines::new(&["blue", "green"]);
        let t = Scratch::new();
        let source = t.join("ours.webm");
        std::fs::copy(f.get("blue"), &source).unwrap();
        let mut cat = catalog(&t.0);
        let (_ours, _) = add(&mut cat, &t.0, &source, "ours", -1).unwrap();
        let server = Transport::new(std::collections::BTreeMap::from([(
            "served".into(),
            f.get("green").into(),
        )]));
        let result = resolve(&mut cat, &t.0, &server, "served").unwrap();
        assert_eq!(result, None);
    }
    // tests.test_video_timeline_comparison.ReconciliationWritesNothingForAForeignClip.test_our_own_clip_still_resolves_to_our_key
    #[test]
    fn test_our_own_clip_still_resolves_to_our_key() {
        let f = Timelines::new(&["blue", "blue"]);
        let t = Scratch::new();
        let source = t.join("ours.webm");
        std::fs::copy(f.get("blue"), &source).unwrap();
        let mut cat = catalog(&t.0);
        let (ours, _) = add(&mut cat, &t.0, &source, "ours", -1).unwrap();
        let server = Transport::new(std::collections::BTreeMap::from([(
            "served".into(),
            f.get("blue").into(),
        )]));
        let result = resolve(&mut cat, &t.0, &server, "served").unwrap();
        assert_eq!(result, Some(ours));
    }
    // tests.test_video_timeline_comparison.ReconciliationWritesNothingForAForeignClip.test_no_foreign_fuid_is_bound_and_it_survives_a_reopen
    #[test]
    fn test_no_foreign_fuid_is_bound_and_it_survives_a_reopen() {
        let f = Timelines::new(&["blue", "green"]);
        let t = Scratch::new();
        let source = t.join("ours.webm");
        std::fs::copy(f.get("blue"), &source).unwrap();
        let mut cat = catalog(&t.0);
        let (_ours, _) = add(&mut cat, &t.0, &source, "ours", -1).unwrap();
        let server = Transport::new(std::collections::BTreeMap::from([(
            "served".into(),
            f.get("green").into(),
        )]));
        let result = resolve(&mut cat, &t.0, &server, "served").unwrap();
        assert_eq!(result, None);
        drop(cat);
        let mut reopened = catalog(&t.0);
        assert_eq!(seen(&mut reopened, "fuid-served"), Value::Null);
        assert_eq!(reopened.all(None).unwrap().len(), 1);
    }
}
