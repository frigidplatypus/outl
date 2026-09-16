//! End-to-end coverage for clearing a page property — the CLI
//! (`outl page prop clear`) and the optional `value` on the
//! `page_prop_set` batch op. The MCP dispatch branch for the same clear
//! (plus the non-string rejection) is pinned in `tests/mcp_smoke.rs`
//! (`page_prop_set_over_mcp_clears_and_rejects_non_string`).
//!
//! The core primitive is `outl_actions::set_property(…, None)` →
//! `Op::SetProp { value: None }`; these tests pin the *surfaces* around
//! it: that an omitted or null value clears, that an empty string sets
//! (it is not a clear), that clearing an absent key is an idempotent
//! no-op, and that the projection drops the `key::` line.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

// ---------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------

/// A workspace in its own `TempDir`, with its own device store.
///
/// The device store is machine-global (`outl-core`'s `DeviceStore`);
/// narrowing `OUTL_DEVICE_DIR` per test keeps parallel runs from seeing
/// each other's actors and keeps the developer's real store clean
/// (root `CLAUDE.md` invariant 9, third question).
struct Ws {
    dir: TempDir,
}

impl Ws {
    /// `outl init` a fresh workspace.
    fn new() -> Self {
        let ws = Ws {
            dir: TempDir::new().expect("tempdir"),
        };
        ws.ok(&["init", ws.root_str().as_str()]);
        ws
    }

    fn root(&self) -> PathBuf {
        self.dir.path().join("ws")
    }

    fn root_str(&self) -> String {
        self.root().to_string_lossy().into_owned()
    }

    /// The projected `.md` for a page slug.
    fn md_path(&self, slug: &str) -> PathBuf {
        self.root().join("pages").join(format!("{slug}.md"))
    }

    fn md_text(&self, slug: &str) -> String {
        fs::read_to_string(self.md_path(slug)).expect("projected .md must exist")
    }

    /// Run `outl` against this workspace's device store. Never asserts.
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_outl"))
            .args(args)
            .env("OUTL_DEVICE_DIR", self.dir.path().join("device"))
            .output()
            .expect("failed to spawn the outl binary")
    }

    /// Run and require success, returning stdout.
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

    /// Run a `--json` subcommand, require success, return its `data`.
    fn json_data(&self, args: &[&str]) -> Value {
        let stdout = self.ok(args);
        let envelope: Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
            panic!(
                "non-JSON stdout for `outl {}`: {e}\n{stdout}",
                args.join(" ")
            )
        });
        envelope["data"].clone()
    }

    /// Create a page through the op log (so it projects a `.md`).
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

    /// Set a page property via the CLI.
    fn set_prop(&self, slug: &str, assignment: &str) -> Value {
        let root = self.root_str();
        self.json_data(&[
            "page",
            "prop",
            "set",
            slug,
            assignment,
            "--json",
            "--workspace",
            root.as_str(),
        ])
    }

    /// Clear a page property via the CLI.
    fn clear_prop(&self, slug: &str, key: &str) -> Value {
        let root = self.root_str();
        self.json_data(&[
            "page",
            "prop",
            "clear",
            slug,
            key,
            "--json",
            "--workspace",
            root.as_str(),
        ])
    }

    /// Read the full envelope for a clear, including expected validation
    /// failures such as attempts to clear page identity keys.
    fn clear_prop_envelope(&self, slug: &str, key: &str) -> Value {
        let root = self.root_str();
        let out = self.run(&[
            "page",
            "prop",
            "clear",
            slug,
            key,
            "--json",
            "--workspace",
            root.as_str(),
        ]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        serde_json::from_str(&stdout).unwrap_or_else(|e| {
            panic!(
                "non-JSON stdout for `outl page prop clear`: {e}\n{stdout}\nstderr: {}",
                String::from_utf8_lossy(&out.stderr)
            )
        })
    }

    /// Read a page property; returns the envelope (may be an error).
    fn get_prop_envelope(&self, slug: &str, key: &str) -> Value {
        let root = self.root_str();
        let out = self.run(&[
            "page",
            "prop",
            "get",
            slug,
            key,
            "--json",
            "--workspace",
            root.as_str(),
        ]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        serde_json::from_str(&stdout).unwrap_or_else(|e| {
            panic!(
                "non-JSON stdout for `outl page prop get`: {e}\n{stdout}\nstderr: {}",
                String::from_utf8_lossy(&out.stderr)
            )
        })
    }

    /// Run a `page_prop_set` batch op with a given `value` JSON fragment
    /// (already-serialized, e.g. `"x"`, `null`, or omitted). Returns the
    /// `data` payload; asserts the run succeeded.
    fn batch_prop_set(&self, slug: &str, key: &str, value_json: Option<&str>) -> Value {
        let args_obj = match value_json {
            Some(v) => format!("{{\"page\":\"{slug}\",\"key\":\"{key}\",\"value\":{v}}}"),
            None => format!("{{\"page\":\"{slug}\",\"key\":\"{key}\"}}"),
        };
        let payload = format!("{{\"ops\":[{{\"op\":\"page_prop_set\",\"args\":{args_obj}}}]}}");
        let envelope = self.run_batch(&payload);
        assert_eq!(
            envelope["ok"], true,
            "batch envelope must be ok: {envelope}"
        );
        envelope["data"].clone()
    }

    /// Spawn `outl batch --ops <payload>` and return the full JSON
    /// envelope without asserting on the outcome (a stop-on-first-error
    /// run is still `ok: true` with a `failed_at`).
    fn run_batch(&self, payload: &str) -> Value {
        let root = self.root_str();
        // Pass the payload as the `--ops` value directly (the reader
        // accepts a literal JSON string, not just `-` / stdin).
        let out = Command::new(env!("CARGO_BIN_EXE_outl"))
            .args([
                "batch",
                "--ops",
                payload,
                "--json",
                "--workspace",
                root.as_str(),
            ])
            .env("OUTL_DEVICE_DIR", self.dir.path().join("device"))
            .output()
            .expect("failed to spawn the outl binary");
        let stdout = String::from_utf8_lossy(&out.stdout);
        serde_json::from_str(&stdout).unwrap_or_else(|e| {
            panic!(
                "non-JSON stdout for `outl batch`: {e}\n{stdout}\nstderr: {}",
                String::from_utf8_lossy(&out.stderr)
            )
        })
    }
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[test]
fn set_then_clear_removes_the_property() {
    let ws = Ws::new();
    ws.create_page("notes");

    ws.set_prop("notes", "status=active");
    assert!(
        ws.md_text("notes").contains("status:: active"),
        "set must project the `key:: value` line"
    );

    let cleared = ws.clear_prop("notes", "status");
    assert_eq!(cleared["key"], "status");
    assert_eq!(cleared["value"], Value::Null);

    // The projection drops the line.
    assert!(
        !ws.md_text("notes").contains("status::"),
        "clear must remove the `status::` line from the .md"
    );

    // A subsequent read reports the property as gone.
    let env = ws.get_prop_envelope("notes", "status");
    assert_eq!(env["ok"], false);
    assert_eq!(env["error"]["code"], "PROP_NOT_FOUND");
}

#[test]
fn clearing_an_absent_key_is_a_noop() {
    let ws = Ws::new();
    ws.create_page("notes");

    // No property was ever set; clearing it must succeed (idempotent),
    // so bulk migrations can clear without first checking for it.
    let cleared = ws.clear_prop("notes", "never-set");
    assert_eq!(cleared["key"], "never-set");
    assert_eq!(cleared["value"], Value::Null);
}

#[test]
fn an_empty_string_is_stored_not_cleared() {
    let ws = Ws::new();
    ws.create_page("notes");

    // `note=` stores `Text("")`; it is a value, not a clear.
    let set = ws.set_prop("notes", "note=");
    assert_eq!(set["value"], "");

    let env = ws.get_prop_envelope("notes", "note");
    assert_eq!(env["ok"], true, "empty string must be readable, not absent");
    assert_eq!(env["data"]["value"], "");
}

#[test]
fn batch_null_value_clears() {
    let ws = Ws::new();
    ws.create_page("notes");
    ws.set_prop("notes", "tag=x");

    let data = ws.batch_prop_set("notes", "tag", Some("null"));
    assert_eq!(data["applied"], 1);
    assert_eq!(data["results"][0]["data"]["value"], Value::Null);

    let env = ws.get_prop_envelope("notes", "tag");
    assert_eq!(env["ok"], false);
    assert_eq!(env["error"]["code"], "PROP_NOT_FOUND");
}

#[test]
fn batch_omitted_value_clears() {
    let ws = Ws::new();
    ws.create_page("notes");
    ws.set_prop("notes", "other=y");

    let data = ws.batch_prop_set("notes", "other", None);
    assert_eq!(data["applied"], 1);
    assert_eq!(data["results"][0]["data"]["value"], Value::Null);

    let env = ws.get_prop_envelope("notes", "other");
    assert_eq!(env["ok"], false);
    assert_eq!(env["error"]["code"], "PROP_NOT_FOUND");
}

#[test]
fn batch_empty_string_sets_not_clears() {
    let ws = Ws::new();
    ws.create_page("notes");

    // A present empty string sets `Text("")`, it does not clear.
    let data = ws.batch_prop_set("notes", "e", Some("\"\""));
    assert_eq!(data["applied"], 1);
    assert_eq!(data["results"][0]["data"]["value"], "");

    let env = ws.get_prop_envelope("notes", "e");
    assert_eq!(env["ok"], true, "empty string must be readable, not absent");
    assert_eq!(env["data"]["value"], "");
}

#[test]
fn projection_drops_only_the_cleared_line() {
    let ws = Ws::new();
    ws.create_page("notes");
    ws.set_prop("notes", "status=active");
    ws.set_prop("notes", "priority=high");

    // Both lines present before the clear.
    let before = ws.md_text("notes");
    assert!(before.contains("status:: active"));
    assert!(before.contains("priority:: high"));

    ws.clear_prop("notes", "status");

    let after = ws.md_text("notes");
    assert!(!after.contains("status::"), "cleared key must be gone");
    assert!(
        after.contains("priority:: high"),
        "untouched key must remain"
    );

    // Exactly zero `key::` lines reference the cleared key.
    let status_lines = after
        .lines()
        .filter(|l| l.trim_start().starts_with("status::"))
        .count();
    assert_eq!(status_lines, 0, "no `status::` line may survive the clear");
}

#[test]
fn structural_page_properties_cannot_be_cleared() {
    let ws = Ws::new();
    ws.create_page("notes");

    for key in ["page-slug", "page-kind", "page-slug::"] {
        let envelope = ws.clear_prop_envelope("notes", key);
        assert_eq!(envelope["ok"], false, "clear must reject {key}");
        assert_eq!(envelope["error"]["code"], "INVALID_ARG");
    }

    // The page remains addressable after each rejected attempt.
    let page = ws.json_data(&[
        "page",
        "get",
        "notes",
        "--json",
        "--workspace",
        ws.root_str().as_str(),
    ]);
    assert_eq!(page["meta"]["slug"], "notes");
}

#[test]
fn batch_non_string_value_is_rejected_not_cleared() {
    let ws = Ws::new();
    ws.create_page("notes");
    ws.set_prop("notes", "status=active");

    // A non-string value is a caller error, not a clear: the op must fail
    // loudly and leave the property intact (a mistyped value must never
    // silently delete it).
    let payload =
        r#"{"ops":[{"op":"page_prop_set","args":{"page":"notes","key":"status","value":42}}]}"#;
    let envelope = ws.run_batch(payload);

    // Stop-on-first-error reports the failure in-band.
    assert_eq!(
        envelope["ok"], true,
        "partial batch is a report, not a hard error: {envelope}"
    );
    assert_eq!(envelope["data"]["failed_at"], 0);
    assert_eq!(envelope["data"]["error"]["code"], "INVALID_ARG");

    // The property survived the rejected op.
    let env = ws.get_prop_envelope("notes", "status");
    assert_eq!(
        env["ok"], true,
        "a rejected clear must not delete the property: {env}"
    );
    assert_eq!(env["data"]["value"], "active");
}
