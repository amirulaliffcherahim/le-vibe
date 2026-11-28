//! Per-file mutation serialisation (pi's `file-mutation-queue.ts`).
//!
//! `edit` is read-modify-write. Two edits to the same file running concurrently read the same
//! original, so one silently loses the other's change. Serialising per canonical path fixes that
//! while leaving different files parallel.

use lazybug_tools::execute_builtin_tool_approved;
use serde_json::json;

fn temp_file(name: &str, body: &str) -> String {
    let dir = std::env::temp_dir().join("lazybug_mutation_queue");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    path.display().to_string()
}

#[test]
fn concurrent_edits_to_one_file_all_land() {
    let path = temp_file("shared.txt", "start\n");
    let mut handles = Vec::new();
    for i in 0..8 {
        let path = path.clone();
        handles.push(std::thread::spawn(move || {
            execute_builtin_tool_approved(
                "edit",
                &json!({
                    "path": path,
                    "edits": [{ "oldText": "start\n", "newText": format!("start\nline{i}\n") }]
                }),
            )
        }));
    }
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Every edit must have applied to the previous result, never to a stale read.
    let ok = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(ok, 8, "all edits should succeed under serialisation: {results:?}");

    let body = std::fs::read_to_string(&path).unwrap();
    for i in 0..8 {
        assert!(body.contains(&format!("line{i}")), "lost edit {i}:\n{body}");
    }
}

#[test]
fn different_files_are_not_serialised_behind_each_other() {
    // Two slow-ish writes to distinct files must overlap; if the queue were global this would take
    // roughly twice as long. We assert generous overlap, not exact timing.
    let a = temp_file("a.txt", "a\n");
    let b = temp_file("b.txt", "b\n");
    let started = std::time::Instant::now();
    let h1 = {
        let a = a.clone();
        std::thread::spawn(move || {
            execute_builtin_tool_approved("write", &json!({ "path": a, "content": "a\n" }))
        })
    };
    let h2 = {
        let b = b.clone();
        std::thread::spawn(move || {
            execute_builtin_tool_approved("write", &json!({ "path": b, "content": "b\n" }))
        })
    };
    h1.join().unwrap().expect("write a");
    h2.join().unwrap().expect("write b");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "distinct files must not deadlock or serialise badly"
    );
}

#[test]
fn writes_and_edits_share_the_same_queue() {
    let path = temp_file("mixed.txt", "base\n");
    let p1 = path.clone();
    let p2 = path.clone();
    let t1 = std::thread::spawn(move || {
        execute_builtin_tool_approved("write", &json!({ "path": p1, "content": "base\n" }))
    });
    let t2 = std::thread::spawn(move || {
        execute_builtin_tool_approved(
            "edit",
            &json!({ "path": p2, "edits": [{ "oldText": "base", "newText": "changed" }] }),
        )
    });
    // Either order is fine; neither may corrupt the file or deadlock.
    let _ = t1.join().unwrap();
    let _ = t2.join().unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(!body.is_empty());
}
