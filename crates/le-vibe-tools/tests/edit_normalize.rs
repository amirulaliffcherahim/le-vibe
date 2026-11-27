//! Edit input normalisation, matching pi's tolerance for what models actually send.
//!
//! Models emit `edits` as an array, as a JSON *string*, as a single object, or as the legacy
//! `oldText`/`newText` pair. Rejecting any of those wastes a whole turn on a formatting mistake.

use le_vibe_tools::execute_builtin_tool_approved;
use serde_json::json;

fn fixture(name: &str, body: &str) -> String {
    let dir = std::env::temp_dir().join("le_vibe_edit_normalize");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    path.display().to_string()
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn canonical_edits_array_still_works() {
    let path = fixture("canonical.txt", "alpha\nbeta\n");
    execute_builtin_tool_approved(
        "edit",
        &json!({ "path": path, "edits": [{ "oldText": "beta", "newText": "BETA" }] }),
    )
    .expect("edit");
    assert!(read(&path).contains("BETA"));
}

#[test]
fn edits_as_a_json_string_is_parsed() {
    let path = fixture("stringified.txt", "alpha\nbeta\n");
    let edits = "[{\"oldText\":\"beta\",\"newText\":\"BETA\"}]";
    execute_builtin_tool_approved(
        "edit",
        &json!({ "path": path, "edits": edits }),
    )
    .expect("a stringified edits array must be accepted");
    assert!(read(&path).contains("BETA"));
}

#[test]
fn edits_as_a_single_object_is_wrapped() {
    let path = fixture("single.txt", "alpha\nbeta\n");
    execute_builtin_tool_approved(
        "edit",
        &json!({ "path": path, "edits": { "oldText": "beta", "newText": "BETA" } }),
    )
    .expect("a single edit object must be accepted");
    assert!(read(&path).contains("BETA"));
}

#[test]
fn legacy_old_and_new_text_pair_is_converted() {
    let path = fixture("legacy.txt", "alpha\nbeta\n");
    execute_builtin_tool_approved(
        "edit",
        &json!({ "path": path, "oldText": "beta", "newText": "BETA" }),
    )
    .expect("legacy oldText/newText must be accepted");
    assert!(read(&path).contains("BETA"));
}

#[test]
fn legacy_pair_combines_with_an_edits_array() {
    let path = fixture("mixed.txt", "one\ntwo\nthree\n");
    execute_builtin_tool_approved(
        "edit",
        &json!({
            "path": path,
            "oldText": "one", "newText": "ONE",
            "edits": [{ "oldText": "three", "newText": "THREE" }]
        }),
    )
    .expect("legacy + array must merge");
    let body = read(&path);
    assert!(body.contains("ONE"), "{body}");
    assert!(body.contains("THREE"), "{body}");
}

#[test]
fn a_json_string_holding_a_single_object_is_wrapped_too() {
    let path = fixture("stringified_single.txt", "alpha\nbeta\n");
    execute_builtin_tool_approved(
        "edit",
        &json!({ "path": path, "edits": "{\"oldText\":\"beta\",\"newText\":\"BETA\"}" }),
    )
    .expect("stringified single object must be accepted");
    assert!(read(&path).contains("BETA"));
}

#[test]
fn real_errors_are_still_reported() {
    let path = fixture("missing.txt", "alpha\n");
    let err = execute_builtin_tool_approved(
        "edit",
        &json!({ "path": path, "edits": [{ "oldText": "nope", "newText": "x" }] }),
    )
    .expect_err("a missing oldText is still an error");
    assert!(err.to_string().contains("not found"), "{err}");

    let dup = fixture("dup.txt", "same\nsame\n");
    let err = execute_builtin_tool_approved(
        "edit",
        &json!({ "path": dup, "edits": [{ "oldText": "same", "newText": "x" }] }),
    )
    .expect_err("a non-unique oldText is still an error");
    assert!(err.to_string().contains("must be unique"), "{err}");
}
