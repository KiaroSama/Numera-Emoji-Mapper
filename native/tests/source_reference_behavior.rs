//! Canonical AST literals and relocation mutations, independent of the implementation's output.
use _native::source_reference_behavior::digest;

#[test]
fn relocation_only_changes_do_not_hide_body_constants_annotations_or_special_literals() {
    let empty = "35ad89e78bf13acf4ca8adea239b5c79db8001f5cbabbf2cfc7e27680e9c3e57";
    for source in [
        "",
        "import os\n",
        "from old import helper\nROOT = arbitrary()\nOFFSET_FILE = root / 'offset'\n",
    ] {
        assert_eq!(digest(source).unwrap(), empty);
    }
    assert_eq!(
        digest("b'\\x00\\xff'\n").unwrap(),
        "1c1488d09dd9f83602f1fd3882d4a36af7ee72d55969360d5df99bae2ce5bb91"
    );
    let original = "import first\nROOT = old()\ndef f(x: int) -> str:\n    from first import helper\n    OFFSET_FILE = old\n    return [b'\\x00\\xff', 1j, ..., x + 1]\n";
    let relocated = "import second\nROOT = new()\ndef f(x: int) -> str:\n    from second import replacement\n    OFFSET_FILE = new\n    return [b'\\x00\\xff', 1j, ..., x + 1]\n";
    let expected = digest(original).unwrap();
    assert_eq!(digest(relocated).unwrap(), expected);
    for changed in [
        original.replace("x + 1", "x + 2"),
        original.replace("x: int", "x: str"),
        original.replace("1j", "2j"),
        original.replace("...", "None"),
        original.replace("\\x00\\xff", "\\x00\\xfe"),
        original.replace("OFFSET_FILE = old", "OTHER_FILE = old"),
    ] {
        assert_ne!(digest(&changed).unwrap(), expected);
    }
    assert!(digest("def broken(").is_err());
    assert!(digest(&" ".repeat(2 * 1024 * 1024 + 1)).is_err());
}
