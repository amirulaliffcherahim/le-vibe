//! Session DAG parity with pi's `session-manager.ts`: the `firstKeptEntryId` compaction boundary,
//! branch summaries, and legacy-file tolerance.

use le_vibe_durable::{EntryKind, SessionEntry, SessionStore};
use std::path::{Path, PathBuf};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("le_vibe_durable_{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn contents(entries: &[SessionEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.content.as_str()).collect()
}

// ---------------------------------------------------------------------------
// 1. Schema: new fields are optional and stay off normal entries
// ---------------------------------------------------------------------------

#[test]
fn legacy_jsonl_without_new_fields_still_loads() {
    let dir = temp_dir("legacy_fields");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("legacy.jsonl");
    // Exactly what pre-P0 le-vibe wrote: no first_kept_entry_id / tokens_before / from_id.
    let legacy = concat!(
        r#"{"id":"msg_1","parent_id":null,"kind":"message","role":"user","content":"hi","timestamp":1700000000}"#,
        "\n",
        r#"{"id":"msg_2","parent_id":"msg_1","kind":"compaction_summary","role":"system","content":"old summary","timestamp":1700000001}"#,
        "\n",
        r#"{"id":"msg_3","parent_id":"msg_2","kind":"message","role":"user","content":"next","timestamp":1700000002}"#,
        "\n"
    );
    std::fs::write(&path, legacy).unwrap();

    let store = SessionStore::open(&path).unwrap();
    assert_eq!(store.entries().len(), 3);
    assert!(store.entries().iter().all(|e| e.first_kept_entry_id.is_none()));
    assert!(store.entries().iter().all(|e| e.tokens_before.is_none()));
    assert!(store.entries().iter().all(|e| e.from_id.is_none()));
    // A legacy compaction has no boundary, so context falls back to summary + everything after.
    assert_eq!(contents(&store.context_entries()), vec!["old summary", "next"]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ordinary_entries_do_not_serialize_the_new_fields() {
    let mut store = SessionStore::in_memory();
    store.append("user", "q").unwrap();
    let json = serde_json::to_string(&store.entries()[0]).unwrap();
    for key in ["first_kept_entry_id", "tokens_before", "from_id"] {
        assert!(
            !json.contains(key),
            "compact entries must omit {key}: {json}"
        );
    }
}

// ---------------------------------------------------------------------------
// 2. `context_entries` = pi's buildContextEntries walk
// ---------------------------------------------------------------------------

#[test]
fn context_entries_keeps_tail_in_place_and_drops_the_summarized_head() {
    let mut store = SessionStore::in_memory();
    let a = store.append("user", "a").unwrap();
    let b = store.append("assistant", "b").unwrap();
    let c = store.append("user", "c").unwrap();
    let d = store.append("assistant", "d").unwrap();
    assert_eq!(c.parent_id.as_deref(), Some(b.id.as_str()));

    // Summarize a..c, keep d and everything after it verbatim.
    store
        .append_compaction_with_keep("SUMMARY", Some(d.id.clone()), Some(3_000))
        .unwrap();
    store.append("user", "e").unwrap();

    let ctx = store.context_entries();
    assert_eq!(
        contents(&ctx),
        vec!["SUMMARY", "d", "e"],
        "kept tail must appear in its original position, not be re-appended"
    );
    // The boundary is recorded on the compaction entry itself.
    let compaction = ctx.iter().find(|e| e.kind == EntryKind::CompactionSummary).unwrap();
    assert_eq!(compaction.first_kept_entry_id.as_deref(), Some(d.id.as_str()));
    assert_eq!(compaction.tokens_before, Some(3_000));

    // `active_branch` keeps its old shape for compatibility (summary + after only).
    assert_eq!(contents(&store.active_branch()), vec!["SUMMARY", "e"]);
    // ...and nothing was duplicated on disk.
    assert_eq!(store.entries().len(), 6);
    let _ = (a, b);
}

#[test]
fn context_entries_with_unresolvable_boundary_keeps_only_the_summary() {
    let mut store = SessionStore::in_memory();
    store.append("user", "a").unwrap();
    store
        .append_compaction_with_keep("SUMMARY", Some("msg_missing".to_string()), None)
        .unwrap();
    store.append("user", "b").unwrap();

    assert_eq!(contents(&store.context_entries()), vec!["SUMMARY", "b"]);
}

#[test]
fn context_entries_drops_pre_compaction_system_messages() {
    let mut store = SessionStore::in_memory();
    store.append_model_change("vibe/mistral-large-4-0").unwrap();
    let keep = store.append("user", "keep-me").unwrap();
    // A system entry *inside* the kept window must still be filtered out of context.
    store.append_mode_change("plan").unwrap();
    store.append("assistant", "kept-reply").unwrap();
    store
        .append_compaction_with_keep("SUMMARY", Some(keep.id.clone()), None)
        .unwrap();

    let ctx = store.context_entries();
    // pi keeps state-only entries (`mode_change`, `model_change`) on the path — they project to
    // zero messages later — and only drops *message* entries whose role is "system".
    assert_eq!(
        contents(&ctx),
        vec!["SUMMARY", "keep-me", "plan", "kept-reply"]
    );
    assert!(
        !ctx.iter().any(|e| e.kind == EntryKind::ModelChange),
        "entries before the boundary are omitted"
    );

    // A pre-boundary system *message* is dropped even when it sits inside the kept window.
    let mut with_system = SessionStore::in_memory();
    let keep = with_system.append("user", "keep").unwrap();
    with_system.append("system", "stale reminder").unwrap();
    with_system.append("assistant", "reply").unwrap();
    with_system
        .append_compaction_with_keep("SUMMARY", Some(keep.id.clone()), None)
        .unwrap();
    assert_eq!(
        contents(&with_system.context_entries()),
        vec!["SUMMARY", "keep", "reply"]
    );
}

// ---------------------------------------------------------------------------
// 3. Branch summaries
// ---------------------------------------------------------------------------

#[test]
fn branch_with_summary_records_the_abandoned_leaf() {
    let mut store = SessionStore::in_memory();
    let root = store.append("user", "root").unwrap();
    let abandoned = store.append("assistant", "abandoned-work").unwrap();

    // Branch back to the root, keeping the abandoned path as a summary.
    store
        .branch_with_summary(Some(&root.id), "we abandoned the other direction")
        .unwrap();

    let summary = store
        .entries()
        .iter()
        .rev()
        .find(|e| e.kind == EntryKind::BranchSummary)
        .expect("branch summary entry");
    assert_eq!(summary.parent_id.as_deref(), Some(root.id.as_str()));
    assert_eq!(summary.from_id.as_deref(), Some(abandoned.id.as_str()));
    assert_eq!(summary.content, "we abandoned the other direction");

    // The branch now continues from the summary.
    assert_eq!(
        contents(&store.context_entries()),
        vec!["root", "we abandoned the other direction"]
    );
}

#[test]
fn branch_with_summary_to_a_new_root_is_allowed() {
    let mut store = SessionStore::in_memory();
    store.append("user", "first").unwrap();
    store.append("assistant", "second").unwrap();

    store.branch_with_summary(None, "restart").unwrap();
    let summary = store.entries().last().unwrap();
    assert_eq!(summary.kind, EntryKind::BranchSummary);
    assert!(summary.parent_id.is_none(), "None branches to a fresh root");
    assert_eq!(summary.from_id.as_deref(), Some("msg_2"));
}

// ---------------------------------------------------------------------------
// 4. Disk round-trip
// ---------------------------------------------------------------------------

#[test]
fn disk_round_trip_preserves_two_branches_and_the_compaction_boundary() {
    let dir = temp_dir("dag_round_trip");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("session.jsonl");

    let (keep_id, after_id) = {
        let mut store = SessionStore::open(&path).unwrap();
        let root = store.append("user", "root").unwrap();
        let keep = store.append("assistant", "kept").unwrap();
        store
            .append_compaction_with_keep("SUMMARY", Some(keep.id.clone()), Some(9_999))
            .unwrap();
        let after = store.append("user", "after-1").unwrap();
        // Second branch off the root: it must NOT inherit the other branch's compaction.
        store.branch_from(&root.id).unwrap();
        store.append("user", "other-branch").unwrap();
        (keep.id.clone(), after.id.clone())
    };

    let reloaded = SessionStore::open(&path).unwrap();
    assert_eq!(
        contents(&reloaded.context_entries()),
        vec!["root", "other-branch"],
        "the active leaf's path has no compaction on it"
    );

    let compaction = reloaded
        .entries()
        .iter()
        .find(|e| e.kind == EntryKind::CompactionSummary)
        .unwrap();
    assert_eq!(compaction.first_kept_entry_id.as_deref(), Some(keep_id.as_str()));
    assert_eq!(compaction.tokens_before, Some(9_999));

    // Walking back onto the compacted branch restores its boundary-aware context.
    let mut walker = reloaded;
    walker.branch_from(&after_id).unwrap();
    assert_eq!(
        contents(&walker.context_entries()),
        vec!["SUMMARY", "kept", "after-1"]
    );
    // `active_branch` keeps the pre-P0 shape on the same path.
    assert_eq!(contents(&walker.active_branch()), vec!["SUMMARY", "after-1"]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn branch_summary_survives_a_disk_reload() {
    let dir = temp_dir("branch_summary_reload");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("session.jsonl");

    let from_id = {
        let mut store = SessionStore::open(&path).unwrap();
        let root = store.append("user", "root").unwrap();
        store.append("assistant", "walk-away").unwrap();
        store.branch_with_summary(Some(&root.id), "abandoned").unwrap();
        store.append("user", "next").unwrap();
        "msg_2".to_string()
    };

    let reloaded = SessionStore::open(&path).unwrap();
    let summary = reloaded
        .entries()
        .iter()
        .find(|e| e.kind == EntryKind::BranchSummary)
        .expect("branch summary survives reload");
    assert_eq!(summary.from_id.as_deref(), Some(from_id.as_str()));
    assert_eq!(contents(&reloaded.context_entries()), vec!["root", "abandoned", "next"]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn convenient_open_helper_keeps_path_semantics() {
    // Guard against a regression where the new append helpers bypass the on-disk writer.
    let dir = temp_dir("writer_parity");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.jsonl");
    {
        let mut store = SessionStore::open(&path).unwrap();
        store
            .append_compaction_with_keep("S", None, Some(1))
            .unwrap();
    }
    let raw = std::fs::read_to_string(&path).unwrap();
    assert_eq!(raw.lines().count(), 1, "helper must persist exactly one entry");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = Path::new("");
}

// ---------------------------------------------------------------------------
// 5. P1: tree projection + labels
// ---------------------------------------------------------------------------

#[test]
fn tree_nodes_build_nested_children_and_mark_the_active_path() {
    let mut store = SessionStore::in_memory();
    let root = store.append("user", "root").unwrap();
    let left = store.append("assistant", "left").unwrap();
    store.branch_from(&root.id).unwrap();
    let right = store.append("assistant", "right").unwrap();

    let tree = store.tree_nodes();
    assert_eq!(tree.len(), 1, "one root");
    assert_eq!(tree[0].entry.id, root.id);
    let kids: Vec<&str> = tree[0].children.iter().map(|c| c.entry.content.as_str()).collect();
    assert_eq!(kids, vec!["left", "right"], "both branches are children");

    // Only the active leaf's ancestry is marked.
    assert!(tree[0].active, "root is on the active path");
    assert!(
        !tree[0].children[0].active,
        "abandoned branch is not active"
    );
    assert!(tree[0].children[1].active, "right is the active leaf");
    let _ = (left, right);
}

#[test]
fn tree_rows_flatten_with_connectors_and_resolved_labels() {
    let mut store = SessionStore::in_memory();
    let root = store.append("user", "root").unwrap();
    store.append("assistant", "only-child").unwrap();
    store.branch_from(&root.id).unwrap();
    store.append("assistant", "second-branch").unwrap();
    store.set_label(&root.id, Some("start")).unwrap();

    let rows = store.tree_rows();
    let labels: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
    assert!(labels[0].contains("start"), "root label resolved: {labels:?}");
    assert!(
        rows.iter().any(|r| r.label.contains("├─") || r.label.contains("└─")),
        "branch connectors expected: {labels:?}"
    );
    // Deterministic order: a single root walks every descendant exactly once.
    assert_eq!(rows.len(), 3, "rows: {labels:?}");
    assert_eq!(rows.iter().filter(|r| r.active).count(), 2, "active chain is root+second-branch");
}

#[test]
fn labels_resolve_to_the_latest_entry_and_can_be_cleared() {
    let mut store = SessionStore::in_memory();
    let node = store.append("user", "hello").unwrap();

    assert_eq!(store.label_for(&node.id), None);
    store.set_label(&node.id, Some("first")).unwrap();
    assert_eq!(store.label_for(&node.id).as_deref(), Some("first"));
    store.set_label(&node.id, Some("second")).unwrap();
    assert_eq!(store.label_for(&node.id).as_deref(), Some("second"));
    // pi stores a label entry with no label to clear it.
    store.set_label(&node.id, None).unwrap();
    assert_eq!(store.label_for(&node.id), None);

    // Label entries never appear as tree nodes themselves.
    assert_eq!(store.tree_nodes()[0].children.len(), 0);
}

#[test]
fn labels_and_tree_survive_a_disk_reload() {
    let dir = temp_dir("labels_reload");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.jsonl");
    let root_id = {
        let mut store = SessionStore::open(&path).unwrap();
        let root = store.append("user", "root").unwrap();
        store.append("assistant", "child").unwrap();
        store.set_label(&root.id, Some("kickoff")).unwrap();
        root.id
    };

    let reloaded = SessionStore::open(&path).unwrap();
    assert_eq!(reloaded.label_for(&root_id).as_deref(), Some("kickoff"));
    let tree = reloaded.tree_nodes();
    assert_eq!(tree[0].label.as_deref(), Some("kickoff"));
    assert_eq!(tree[0].children.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tree_nodes_skips_state_only_entries_but_keeps_their_children() {
    let mut store = SessionStore::in_memory();
    let root = store.append("user", "root").unwrap();
    store.append_tool_call("tc_1", "read", r#"{"path":"a.rs"}"#).unwrap();
    store.append_tool_result("tc_1", "read", "1: fn main()").unwrap();
    let reply = store.append("assistant", "done").unwrap();

    let tree = store.tree_nodes();
    // The tool call/result sit between root and reply but must not become tree nodes.
    assert_eq!(tree[0].entry.id, root.id);
    assert_eq!(tree[0].children.len(), 1);
    assert_eq!(tree[0].children[0].entry.id, reply.id);

    // ...and the tool result entry is still reachable through the DAG.
    assert!(store.entries().iter().any(|e| e.kind == EntryKind::ToolResult));
}

// ---------------------------------------------------------------------------
// 6. Rewind checkpoints (DAG filtering stays in durable)
// ---------------------------------------------------------------------------

#[test]
fn rewind_checkpoints_list_only_user_messages_in_order() {
    let mut store = SessionStore::in_memory();
    store.append("user", "first question").unwrap();
    store.append("assistant", "answer").unwrap();
    store.append_tool_call("tc_1", "read", r#"{"path":"a.rs"}"#).unwrap();
    store.append_tool_result("tc_1", "read", "body").unwrap();
    store.append("user", "second\nquestion").unwrap();
    store.append_model_change("vibe/x").unwrap();

    let rows = store.rewind_checkpoints();
    assert_eq!(rows.len(), 2, "only user messages: {rows:?}");
    assert_eq!(rows[0].label, "#1 user");
    assert_eq!(rows[1].label, "#2 user");
    assert_eq!(rows[0].preview, "first question");
    assert_eq!(rows[1].preview, "second question", "newlines flattened");
    assert!(rows.iter().all(|r| !r.id.is_empty()));
}

#[test]
fn rewind_checkpoints_ignore_compaction_summaries() {
    let mut store = SessionStore::in_memory();
    store.append("user", "q1").unwrap();
    store.append("assistant", "a1").unwrap();
    store.append_compaction("SUMMARY").unwrap();
    store.append("user", "q2").unwrap();

    let rows = store.rewind_checkpoints();
    assert_eq!(rows.len(), 2, "summary is system, not a checkpoint");
}
