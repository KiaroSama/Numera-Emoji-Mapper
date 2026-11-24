use super::required_step_run;

pub(super) fn windows_media_prerequisites(workflow: &str, tests: &str) -> Result<(), &'static str> {
    let heading = "- name: Install Windows video prerequisites";
    let lines = workflow.lines().collect::<Vec<_>>();
    let starts = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim() == heading)
        .collect::<Vec<_>>();
    let [(start, line)] = starts.as_slice() else {
        return Err("Windows media installation must exist exactly once");
    };
    let indentation = line.len() - line.trim_start().len();
    let mut windows = false;
    let mut shared = false;
    for line in lines[start + 1..].iter().take_while(|line| {
        line.trim().is_empty()
            || line.trim_start().starts_with('#')
            || line.len() - line.trim_start().len() > indentation
    }) {
        if line.trim().is_empty()
            || line.trim_start().starts_with('#')
            || line.len() - line.trim_start().len() != indentation + 2
        {
            continue;
        }
        let (key, value) = line
            .trim()
            .split_once(':')
            .ok_or("unsupported Windows media step metadata")?;
        let key = key.trim().trim_matches(['\'', '"']);
        let value = value.split('#').next().unwrap().trim();
        match key {
            "if" if value == "runner.os == 'Windows'" && !windows => windows = true,
            "uses" if value == "./.github/actions/windows-video-tools" && !shared => {
                shared = true;
            }
            "if" | "uses" | "run" => return Err("Windows media must use the shared action"),
            "continue-on-error" if value != "false" => {
                return Err("Windows media installation failure must fail the job");
            }
            _ => {}
        }
    }
    if !windows || !shared {
        return Err("Windows media must use the shared action on Windows");
    }
    let versions = required_step_run(workflow, "Require actual video tools")?;
    for command in ["ffmpeg -version", "ffprobe -version"] {
        if !versions.lines().any(|line| line == command) {
            return Err("both actual media tools must execute before native tests");
        }
    }
    required_step_run(workflow, tests)?;
    let install = workflow.find(heading).unwrap();
    let versions = workflow.find("- name: Require actual video tools").unwrap();
    let tests = workflow.find(&format!("- name: {tests}")).unwrap();
    if !(install < versions && versions < tests) {
        return Err("media installation and version checks must precede native tests");
    }
    Ok(())
}

#[test]
fn both_full_native_workflows_share_required_windows_media_installation() {
    for (workflow, tests) in [
        (
            include_str!("../../../.github/workflows/ci.yml")
                .split_once("\n  worker:")
                .unwrap()
                .0,
            "All Rust tests through the isolated runner",
        ),
        (
            include_str!("../../../.github/workflows/native-checkpoint.yml")
                .split_once("\n  focused:")
                .unwrap()
                .0,
            "Full isolated test discovery",
        ),
    ] {
        windows_media_prerequisites(workflow, tests).unwrap();
        for (old, new) in [
            (
                "Install Windows video prerequisites",
                "Missing Windows media",
            ),
            ("if: runner.os == 'Windows'", "if: false"),
            (
                "uses: ./.github/actions/windows-video-tools",
                "uses: ./.github/actions/test-network",
            ),
            (
                "uses: ./.github/actions/windows-video-tools",
                "run: echo skipped",
            ),
            (
                "uses: ./.github/actions/windows-video-tools",
                "uses: ./.github/actions/windows-video-tools\n        continue-on-error: true",
            ),
            ("ffprobe -version", "echo ffprobe -version"),
        ] {
            assert!(workflow.contains(old));
            assert!(
                windows_media_prerequisites(&workflow.replace(old, new), tests).is_err(),
                "{tests}: {old} -> {new}"
            );
        }
        let heading = "      - name: Require actual video tools\n";
        let (before, after) = workflow.split_once(heading).unwrap();
        let (versions, after) = after.split_once("      - ").unwrap();
        let reordered = format!("{before}      - {after}\n{heading}{versions}");
        assert!(windows_media_prerequisites(&reordered, tests).is_err());
    }
}
