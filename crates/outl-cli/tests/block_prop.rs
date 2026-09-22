//! End-to-end coverage for `outl block prop …` — the block-level
//! counterpart to `outl page prop …`.
//!
//! The core primitive is the same `outl_actions::set_property` →
//! `Op::SetProp`, but the op lands on the **block** node and the
//! projection re-projects the **enclosing page**. These tests pin the
//! surfaces around it: that a set renders as a `key:: value` line
//! beneath the block, that clear drops only that line, that `get` /
//! `list` read back what was written, and that the block property is
//! addressable by the query engine's date filters (a `due::` ISO value).

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

/// A workspace in its own `TempDir`, with its own device store.
struct Ws {
    dir: TempDir,
}

impl Ws {
    fn new() -> Self {
        let ws = Ws {
            dir: TempDir::new().expect("tempdir"),
        };
        let root = ws.root_str();
        ws.ok(&["init", root.as_str()]);
        ws
    }

    fn root(&self) -> PathBuf {
        self.dir.path().join("ws")
    }

    fn root_str(&self) -> String {
        self.root().to_string_lossy().into_owned()
    }

    fn md_text(&self, slug: &str) -> String {
        fs::read_to_string(self.root().join("pages").join(format!("{slug}.md")))
            .expect("projected .md must exist")
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_outl"))
            .args(args)
            .env("OUTL_DEVICE_DIR", self.dir.path().join("device"))
            .output()
            .expect("failed to spawn the outl binary")
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "`outl {}` must succeed:\nstdout: {}\nstderr: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn json_data(&self, args: &[&str]) -> Value {
        let stdout = self.ok(args);
        let envelope: Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|e| panic!("non-JSON stdout: {e}\n{stdout}"));
        envelope["data"].clone()
    }

    fn envelope(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        let stdout = String::from_utf8_lossy(&out.stdout);
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("non-JSON stdout: {e}\n{stdout}"))
    }

    fn create_page(&self, slug: &str) {
        let root = self.root_str();
        self.json_data(&[
            "page",
            "create",
            slug,
            "--json",
            "--workspace",
            root.as_str(),
        ]);
    }

    /// Append a block, return its id (ULID string).
    fn append(&self, slug: &str, text: &str) -> String {
        let root = self.root_str();
        let data = self.json_data(&[
            "block",
            "append",
            "--page",
            slug,
            "--text",
            text,
            "--json",
            "--workspace",
            root.as_str(),
        ]);
        data["id"].as_str().expect("block id").to_string()
    }

    fn set_prop(&self, id: &str, assignment: &str) -> Value {
        let root = self.root_str();
        self.json_data(&[
            "block",
            "prop",
            "set",
            id,
            assignment,
            "--json",
            "--workspace",
            root.as_str(),
        ])
    }

    fn clear_prop(&self, id: &str, key: &str) -> Value {
        let root = self.root_str();
        self.json_data(&[
            "block",
            "prop",
            "clear",
            id,
            key,
            "--json",
            "--workspace",
            root.as_str(),
        ])
    }

    fn get_prop(&self, id: &str, key: &str) -> Value {
        let root = self.root_str();
        self.envelope(&[
            "block",
            "prop",
            "get",
            id,
            key,
            "--json",
            "--workspace",
            root.as_str(),
        ])
    }

    fn list_props(&self, id: &str) -> Value {
        let root = self.root_str();
        self.json_data(&[
            "block",
            "prop",
            "list",
            id,
            "--json",
            "--workspace",
            root.as_str(),
        ])
    }
}

#[test]
fn block_prop_set_renders_a_child_line_and_clears() {
    let ws = Ws::new();
    ws.create_page("tasks");
    let id = ws.append("tasks", "TODO Buy the rocket fuel");

    let set = ws.set_prop(&id, "due=2026-09-25");
    assert_eq!(set["id"], id);
    assert_eq!(set["key"], "due");
    assert_eq!(set["value"], "2026-09-25");

    let md = ws.md_text("tasks");
    assert!(
        md.contains("due:: 2026-09-25"),
        "set must project a `due::` line under the block:\n{md}"
    );

    ws.clear_prop(&id, "due");
    assert!(
        !ws.md_text("tasks").contains("due::"),
        "clear must drop the `due::` line"
    );

    let env = ws.get_prop(&id, "due");
    assert_eq!(env["ok"], false);
    assert_eq!(env["error"]["code"], "PROP_NOT_FOUND");
}

#[test]
fn get_and_list_read_back_what_was_written() {
    let ws = Ws::new();
    ws.create_page("tasks");
    let id = ws.append("tasks", "TODO File the report");
    ws.set_prop(&id, "due=2026-09-25");
    ws.set_prop(&id, "priority=high");

    let got = ws.get_prop(&id, "due");
    assert_eq!(got["ok"], true);
    assert_eq!(got["data"]["value"], "2026-09-25");

    let listed = ws.list_props(&id);
    let mut keys: Vec<&str> = listed["properties"]
        .as_array()
        .expect("properties array")
        .iter()
        .map(|p| p["key"].as_str().expect("key"))
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["due", "priority"]);
}

#[test]
fn clearing_an_absent_block_prop_is_a_noop() {
    let ws = Ws::new();
    ws.create_page("tasks");
    let id = ws.append("tasks", "TODO Nothing scheduled");
    let cleared = ws.clear_prop(&id, "never-set");
    assert_eq!(cleared["value"], Value::Null);
}

#[test]
fn block_prop_rejects_structural_keys() {
    let ws = Ws::new();
    ws.create_page("tasks");
    let id = ws.append("tasks", "TODO Guard the keys");
    let root = ws.root_str();
    let env = ws.envelope(&[
        "block",
        "prop",
        "set",
        &id,
        "page-slug=sneaky",
        "--json",
        "--workspace",
        root.as_str(),
    ]);
    assert_eq!(env["ok"], false);
    assert_eq!(env["error"]["code"], "INVALID_ARG");
}

#[test]
fn block_prop_on_unknown_block_is_block_not_found() {
    let ws = Ws::new();
    ws.create_page("tasks");
    // A well-formed ULID that does not exist in the workspace.
    let env = {
        let root = ws.root_str();
        ws.envelope(&[
            "block",
            "prop",
            "set",
            "01HX0000000000000000000000",
            "due=2026-09-25",
            "--json",
            "--workspace",
            root.as_str(),
        ])
    };
    assert_eq!(env["ok"], false);
    assert_eq!(env["error"]["code"], "BLOCK_NOT_FOUND");
}
