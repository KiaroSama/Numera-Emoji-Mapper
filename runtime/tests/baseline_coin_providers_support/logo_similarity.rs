use super::*;

mod logo_review_distance {
    use super::*;
    // tests.test_verify_logos.LogoReviewDistance.test_the_same_mark_inverted_is_not_reported_as_different
    #[test]
    fn test_the_same_mark_inverted_is_not_reported_as_different() {
        let theirs = noise("some-coin");
        let ours = noise("some-coin-inverted");
        let plain = (rt::codec_image::provider_dhash(&ours).unwrap()
            ^ rt::codec_image::provider_dhash(&theirs).unwrap())
        .count_ones();
        assert!(plain > 20, "fixture must be inversion-sensitive");
        assert!(rt::codec_image::logo_distance(&ours, &theirs).unwrap() <= 8);
    }
    // tests.test_verify_logos.LogoReviewDistance.test_a_genuinely_different_image_is_still_far
    #[test]
    fn test_a_genuinely_different_image_is_still_far() {
        assert!(
            rt::codec_image::logo_distance(&noise("solana"), &noise("solama-the-llama")).unwrap()
                > 20
        );
    }
}
