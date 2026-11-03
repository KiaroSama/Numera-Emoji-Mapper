//! Exact source and asset admission for the one historical panel selector.
use crate::source_git_inventory::Source;
use std::path::Path;

pub(crate) const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
pub(crate) const IDENTITIES: [(&str, &str); 36] = [
    (
        "tests/test_panel_view.py",
        "d571bf744c60ccc51ebabccab5dcb0333076f096fb5d2ec825bd3829803a1646",
    ),
    (
        "tests/reference/panel_view.py",
        "1f01c18d1a7ed38369eaef69c3c82a4c8956c76bfb578e75c9681c5c30cc94c8",
    ),
    (
        "tests/reference/catalog.py",
        "5dab2e475754dc219dfe3fd8e98012fffd59ab0c760f646752ff9d35f7188ee2",
    ),
    (
        "emojikit/media_paths.py",
        "0947331cd018503884b2a3c0b33f401476f9b69a0f4865ee337b2d4d90fa0ff3",
    ),
    (
        "tests/reference/ingest.py",
        "6654f129fba59b600d8498c7d734a36973fd9761276075d1f23a28974ba69162",
    ),
    (
        "tests/_similarity_oracle.py",
        "9be48166b64ab2052798c67e2f7426c2dd0008fc27b4d668a785877fba7ac44e",
    ),
    (
        "emojikit/errors.py",
        "6c29243e2ec976606174605f1deaa3d23ef8d94fea47e886465c27f4f21f2026",
    ),
    (
        "emojikit/identity.py",
        "8cbefd550e306e671df1e52af2dd9fe6b4e3647c3aa1b13dc98bf35080d36e89",
    ),
    (
        "emojikit/media.py",
        "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53",
    ),
    (
        "emojikit/operator_config.py",
        "a19f804a7bdd37baccb474eac96df6aa4d46439dd956e457c06adf3c180b9591",
    ),
    (
        "tests/_panel_fixtures.py",
        "15f432b0d9063e0d83b7ea3bbd90f913e14c2c74818b7ba8ef92af50398d4039",
    ),
    (
        "tests/reference/panel.py",
        "569f53b440e3c797146f3ed4b6396cf50091e77a12ca2f53bc414e2b0e23ba35",
    ),
    (
        "tests/test_panel_assets.py",
        "2e25006a84e81c82d2d195cad86fccce002527619b81db02a14e4000aa35b2a2",
    ),
    (
        "tests/test_panel_plan.py",
        "827a4316c34258639689ba69271499219ae1d13be6af8cf389d8f4027b611b08",
    ),
    (
        "tests/reference/panel_plan.py",
        "14d4df245dbb13a13c768f405ba0217b2ea885b204d19c2c4d8a691177a7100e",
    ),
    (
        "tests/reference/packstate.py",
        "d6539619a473dd86d7706bb929a8cad759d29639cd8006a1e083a0504693f9e0",
    ),
    (
        "tests/test_panel_page_actions.py",
        "a2d1d79a1f748b7e7eed95465aab8f276b6d34be1cd2286b447a1d66d3208e32",
    ),
    (
        "tests/test_panel_page_grid.py",
        "ddf702799d8edbbaf06b133646285954e73f67d97f2b344b5f1b5c37d550e18b",
    ),
    (
        "tests/_panel_page_fixtures.py",
        "246557861e0df7dad06fa099c2ce8be7fa2da073361482dc77ac14c672790a3d",
    ),
    (
        "tests/reference/collection_state.py",
        "b8bf2695954cbe61a3e3cb68c8da0c583aa80a1b645abba01eabe9a28776074a",
    ),
    (
        "tests/test_panel_logging.py",
        "6d512aafb59c2879302d4c8d49ca2f2569ab0d8c6855fcad07cd0caf6bd6df13",
    ),
    (
        "tests/reference/panel_logging.py",
        "e0ca46502251fbc326a46948f68c46f041d4f786b7ea1fe4f11cee527b327959",
    ),
    (
        "tests/test_panel.py",
        "a9c15877f7321c107a879ba94860e3b1b0a112430cf52dd9b15c440f22cae15e",
    ),
    (
        "tests/test_panel_server.py",
        "6d8bb624b97fcb363be882e0ad64826321778f776b413ade6f61af42f8996734",
    ),
    (
        "tests/reference/script_json.py",
        "b9afcea858593c9e532ee3690038433e48eefc00910af603b6b1e8b90a3e56e4",
    ),
    (
        "tests/test_panel_preview.py",
        "b8ccdca30da5cf00e1f66f7ef325b8a1f8b4cf22b0d7ba91531a9e0ae15b07f2",
    ),
    (
        "tests/reference/panel_preview.py",
        "832548b52fdc4862ca02d8e7d4af9eae84751ef590f333c67f1dd483e9d26024",
    ),
    (
        "tests/test_panel_save_scope.py",
        "85689ad729712538dfdc50cb2f81e3ab64651aecd20e6e4996912b5e1c790934",
    ),
    (
        "tests/reference/panel_save.py",
        "3960caaf41cb2b1c768cf72160d74e6df09c47f85cfd1f8496e0a2b6a2bdc760",
    ),
    (
        "emojikit/state_artifacts.py",
        "3b0faf872a3d7ee9ebfd8324060e7c2ff7541ead4b743fa30e5ebd9939248001",
    ),
    (
        "emojikit/media_lottie.py",
        "5112b55f21d4d241ffe9660d5294de20487d4ea1e9772188fb6bd125f7fef63b",
    ),
    (
        "tests/test_panel_intent_persistence.py",
        "e56d74b87720749b2d66951dc94e962b97d0945a3b2ce40f975fd4dd39779f1f",
    ),
    (
        "tests/reference/maintenance.py",
        "d145a381f6da57a03551eeec76d1812e5e521af707e6dc7e7925dc0852b86278",
    ),
    (
        "tests/reference/migration_bundle.py",
        "c42b661d5e88f64a00780d95cbf88bbf38647c2ae2c661e25c7bd420a2cc0241",
    ),
    (
        "tests/test_panel_guard.py",
        "c8e578393b0a23f36b46ae488099b07552bddabb006e884d2c9bca753edc5236",
    ),
    (
        "tests/test_panel_sandbox.py",
        "0fa7d36a1f002949dede93a62c48655ece026ee76a3f6d665b3d40618c97481a",
    ),
];
pub(crate) const ASSETS: [(&str, &str); 7] = [
    (
        "panel-grid.js",
        "9d736a6d624e47211ccd6e3a477fcc2a0ace86231e48914d203cc15699d98f58",
    ),
    (
        "panel-motion.js",
        "45736ed81a1b393f3bb27f8524b2d411ab3b43d3db0851e732cfae2cc3bc7cac",
    ),
    (
        "panel-save.js",
        "a768b90ea70d777f1767d0cd94e097ec8c9c19d50563b60fa8aa7613c6c430cd",
    ),
    (
        "panel-drag.js",
        "77fbeb4a113a03d664c1a212cc84e81d536176a08968cdd209010a4696c143cd",
    ),
    (
        "panel-actions.js",
        "91f016455e68b6cf7af2df9ea37ec75395f6367d624f192bbb016b45855287b8",
    ),
    (
        "panel-holding.js",
        "04557214314e2a1b694ccac068a6b635b3ff0020e87a9beb68a6bdedaae4642f",
    ),
    (
        "panel-draft.js",
        "8a7fd7f4afdcea1bdc8ecf03f739c5cbb0c90eef9f32c087431a087a8be24b2b",
    ),
];
pub fn source_paths() -> [&'static str; 36] {
    IDENTITIES.map(|(path, _)| path)
}
pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("panel view replay requires exactly thirty-six pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("panel view source identity drift; refusing execution".into());
        }
    }
    Ok(())
}
pub(crate) fn script(root: &Path) -> Result<String, String> {
    let mut parts = Vec::new();
    for (name, hash) in ASSETS {
        let path = root.join("assets").join(name);
        crate::tooling_environment::reject_links(
            &path,
            "original panel assets cannot follow links",
        )?;
        let text = std::fs::read_to_string(path)
            .map_err(|_| "cannot read original panel script")?
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        if crate::codec_raster::digest_hex(text.as_bytes()) != hash {
            return Err("original panel asset identity drift".into());
        }
        parts.push(text);
    }
    for (name, expected) in [
        (
            "panel.html",
            "591cd8887e85bdc4fdd0fb85bf477c32153a7527b84d1ce45613c316cfa9a151",
        ),
        (
            "logo-128.png",
            "1cbdcacaa254f2361fc8c404a9edbfbb528f19a3f7467987e44c7a2730340d83",
        ),
    ] {
        let path = root.join("assets").join(name);
        crate::tooling_environment::reject_links(
            &path,
            "original panel assets cannot follow links",
        )?;
        let bytes = if name.ends_with(".html") {
            std::fs::read_to_string(path)
                .map_err(|_| "cannot read original panel page")?
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                .into_bytes()
        } else {
            std::fs::read(path).map_err(|_| "cannot read original panel icon")?
        };
        if crate::codec_raster::digest_hex(&bytes) != expected {
            return Err("original panel asset identity drift".into());
        }
    }
    Ok(parts.join("\n"))
}
