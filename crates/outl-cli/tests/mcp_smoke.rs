//! Smoke test for the MCP stdio surface.
//!
//! Spawns `outl mcp serve --workspace <tmp>` in a subprocess, sends
//! `initialize`, `tools/list`, and `tools/call outl_workspace_info`
//! through stdin, and asserts the JSON-RPC responses. This is the
//! ground truth — if Claude Desktop / Cursor break, this would too.

use serde_json::json;
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use tempfile::TempDir;

fn outl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_outl"))
}

fn init_workspace() -> TempDir {
    let dir = TempDir::new().unwrap();
    let status = outl()
        .arg("init")
        .arg(dir.path())
        .status()
        .expect("init failed");
    assert!(status.success());
    dir
}

struct McpClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl McpClient {
    fn spawn(workspace: &std::path::Path) -> Self {
        let mut child = outl()
            .args(["--workspace"])
            .arg(workspace)
            .args(["mcp", "serve"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn mcp serve");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn call(&mut self, payload: Value) -> Value {
        let line = payload.to_string();
        writeln!(self.stdin, "{line}").unwrap();
        self.stdin.flush().unwrap();
        let mut response = String::new();
        self.stdout.read_line(&mut response).expect("read response");
        serde_json::from_str(response.trim()).expect("response was JSON")
    }
}

/// Parse the data payload out of a **successful** `tools/call` result.
///
/// Success replies are content-only (no `structuredContent` at this
/// protocol version); the data lives as compact JSON in
/// `content[0].text`. Markdown-first tools put raw `.md` there instead,
/// so this is only for the JSON-shaped tools.
fn success_data(result: &Value) -> Value {
    assert_eq!(
        result["isError"], false,
        "expected a success reply: {result}"
    );
    let text = result["content"][0]["text"]
        .as_str()
        .expect("content[0].text is a string");
    serde_json::from_str(text).expect("success content is JSON")
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Closing stdin makes the MCP loop exit.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn initialize_then_call_workspace_info() {
    let ws = init_workspace();
    let mut client = McpClient::spawn(ws.path());

    let init = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {}
        }
    }));
    assert_eq!(init["id"], 1);
    assert_eq!(init["result"]["serverInfo"]["name"], "outl");

    let tools = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list"
    }));
    let list = tools["result"]["tools"]
        .as_array()
        .expect("tools list is an array");
    assert!(
        list.iter().any(|t| t["name"] == "outl_workspace_info"),
        "outl_workspace_info must be registered"
    );

    let call = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "outl_workspace_info",
            "arguments": {}
        }
    }));
    assert_eq!(call["id"], 3);
    let data = success_data(&call["result"]);
    assert!(data["root"].is_string());
}

#[test]
fn doctor_via_mcp_does_not_lie_about_lock() {
    // Regression: doctor used to call `WorkspaceLock::acquire`, which
    // would always fail inside the MCP session (the server already
    // owns the lock) and report a non-existent contention. The fix
    // skips the lock probe and emits an info-level "probe skipped"
    // finding instead.
    let ws = init_workspace();
    let mut client = McpClient::spawn(ws.path());

    let _ = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {} }
    }));

    let resp = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "outl_workspace_doctor", "arguments": {} }
    }));
    let data = success_data(&resp["result"]);
    let findings = data["findings"].as_array().unwrap();
    let has_lock_warning = findings.iter().any(|f| {
        f["message"]
            .as_str()
            .unwrap_or("")
            .contains("another outl process is holding the workspace lock")
    });
    assert!(
        !has_lock_warning,
        "doctor must not warn about its own lock when running in MCP session"
    );
}

#[test]
fn resources_read_after_tool_call_does_not_deadlock_on_lock() {
    // Regression: resources/read used to call `ws::open` directly,
    // which re-acquires the workspace lock. After any tool call had
    // already cached the workspace through `ServerCtx`, the lock was
    // held for the session and the resource read would fail with
    // `LockError::AlreadyHeld`. The fix routes everything through
    // `ctx.with_workspace`, reusing the cached `WsCtx`.
    let ws = init_workspace();
    let mut client = McpClient::spawn(ws.path());

    let _ = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {} }
    }));

    // Warm the cached workspace via a tool call.
    let _ = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "outl_workspace_info", "arguments": {} }
    }));

    // Now read a resource — used to deadlock against the cached lock.
    let read = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "resources/read",
        "params": { "uri": "outl://workspace/info" }
    }));
    assert!(
        read.get("error").is_none(),
        "resources/read after tool call must not fail: {read}"
    );
    assert_eq!(read["id"], 3);
    let contents = read["result"]["contents"]
        .as_array()
        .expect("resources/read returns a contents array");
    assert!(!contents.is_empty());
    assert_eq!(contents[0]["uri"], "outl://workspace/info");
}

#[test]
fn page_create_then_get_via_mcp() {
    let ws = init_workspace();
    let mut client = McpClient::spawn(ws.path());

    // The handshake is required by some hosts; harmless if skipped, but
    // we go through it so the test mirrors real usage.
    let _ = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {} }
    }));

    let create = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "outl_page_create",
            "arguments": { "slug": "ideas", "title": "Ideas" }
        }
    }));
    let data = success_data(&create["result"]);
    assert_eq!(data["meta"]["slug"], "ideas");

    let get = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "outl_page_get",
            "arguments": { "slug": "ideas" }
        }
    }));
    let data = success_data(&get["result"]);
    assert_eq!(data["meta"]["title"], "Ideas");
}

/// `outl_page_prop_set` doubles as the clear path: an omitted or null
/// `value` removes the property, and a present non-string must be
/// rejected rather than silently clearing it. This drives the real
/// server so the dispatch branch — not just the shared handler that
/// `tests/prop_clear.rs` pins through the CLI and batch — is covered.
#[test]
fn page_prop_set_over_mcp_clears_and_rejects_non_string() {
    let ws = init_workspace();
    let mut client = McpClient::spawn(ws.path());

    let _ = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {} }
    }));

    let create = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "outl_page_create",
            "arguments": { "slug": "notes" }
        }
    }));
    assert_eq!(create["result"]["isError"], false);

    // Set a property.
    let set = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "outl_page_prop_set",
            "arguments": { "page": "notes", "key": "status", "value": "active" }
        }
    }));
    assert_eq!(success_data(&set["result"])["value"], "active");

    // A null value clears it.
    let clear_null = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "outl_page_prop_set",
            "arguments": { "page": "notes", "key": "status", "value": null }
        }
    }));
    assert_eq!(success_data(&clear_null["result"])["value"], Value::Null);

    // Re-set, then clear by omitting the value entirely.
    let reset = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "outl_page_prop_set",
            "arguments": { "page": "notes", "key": "status", "value": "active" }
        }
    }));
    assert_eq!(success_data(&reset["result"])["value"], "active");

    let clear_omit = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": {
            "name": "outl_page_prop_set",
            "arguments": { "page": "notes", "key": "status" }
        }
    }));
    assert_eq!(success_data(&clear_omit["result"])["value"], Value::Null);

    // The property is gone.
    let get = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": {
            "name": "outl_page_prop_get",
            "arguments": { "page": "notes", "key": "status" }
        }
    }));
    assert_eq!(get["result"]["isError"], true);
    assert_eq!(
        get["result"]["structuredContent"]["error"]["code"],
        "PROP_NOT_FOUND"
    );

    // A non-string value is rejected, not silently cleared.
    let bad = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": {
            "name": "outl_page_prop_set",
            "arguments": { "page": "notes", "key": "status", "value": 42 }
        }
    }));
    assert_eq!(bad["result"]["isError"], true);
    assert_eq!(
        bad["result"]["structuredContent"]["error"]["code"],
        "INVALID_ARG"
    );
}

/// The whole point of reading a journal over MCP is being able to act on
/// what you read. That needs a block id, and a rendered `.md` has none —
/// ids live in the sidecar, never in the markdown.
///
/// This drives the real server, so it fails if `outl_daily_today` is ever
/// flattened to its `md` field again. The unit test in
/// `mcp/tools/payload.rs` pins the projection; this one pins that the
/// resulting id actually works as a write target.
#[test]
fn daily_today_over_mcp_returns_ids_a_write_tool_can_use() {
    let ws = init_workspace();
    let mut client = McpClient::spawn(ws.path());

    let _ = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {} }
    }));

    let _ = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "outl_daily_append",
            "arguments": { "text": "TODO ship the token diet" }
        }
    }));

    let today = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "outl_daily_today", "arguments": {} }
    }));
    let data = success_data(&today["result"]);

    assert!(
        data["date"].is_string(),
        "outl_daily_today takes no argument, so its reply is the only \
         thing naming the journal it opened: {data}"
    );
    assert!(
        data["md"]
            .as_str()
            .unwrap_or_default()
            .contains("ship the token diet"),
        "the markdown must still be there: {data}"
    );

    let block_id = data["outline"][0]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the journal outline must carry block ids: {data}"))
        .to_string();

    // The id is only worth returning if it is a usable write target.
    let toggled = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "outl_block_toggle_todo",
            "arguments": { "id": block_id }
        }
    }));
    assert_eq!(
        toggled["result"]["isError"], false,
        "the id read back from the journal must work as a write target: {toggled}"
    );
}

/// `outl_export_json` exists to be an interchange format, so its payload
/// has to deserialize back into the type that produced it.
///
/// Its `blocks` are `outl_md::OutlineNode`s, whose `properties` field has
/// no `#[serde(default)]`. Any projection that drops an empty
/// `properties` breaks this on every block without one — silently, since
/// the JSON still looks fine to a reader.
#[test]
fn export_json_over_mcp_round_trips_into_the_parser_ast() {
    let ws = init_workspace();
    let mut client = McpClient::spawn(ws.path());

    let _ = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {} }
    }));

    let _ = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "outl_page_create",
            "arguments": { "slug": "export-me", "title": "Export me" }
        }
    }));
    let _ = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "outl_block_append",
            "arguments": { "page": "export-me", "text": "a block with no properties" }
        }
    }));

    let export = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "outl_export_json",
            "arguments": { "slug": "export-me" }
        }
    }));
    let data = success_data(&export["result"]);

    let blocks: Vec<outl_md::OutlineNode> = serde_json::from_value(data["blocks"].clone())
        .unwrap_or_else(|e| {
            panic!("export_json must deserialize back into the parser AST ({e}): {data}")
        });
    assert!(
        blocks.iter().any(|b| b.text.contains("no properties")),
        "the exported block must survive the projection: {data}"
    );
}

/// RFC 0255 Part 1: a page that stopped syncing must come back as a
/// distinct, structured refusal, not a generic `INTERNAL` failure —
/// the caller needs to be able to tell "this page stopped syncing"
/// from "that operation failed" without parsing prose.
#[test]
fn frozen_page_update_returns_structured_refusal_not_a_generic_error() {
    let ws = init_workspace();

    // Create the page through the MCP itself so its `.md` + sidecar
    // start out consistent, then drop the client (closing stdin ends
    // the `mcp serve` process and releases the workspace lock) before
    // hand-corrupting the file on disk.
    {
        let mut client = McpClient::spawn(ws.path());
        let _ = client.call(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "protocolVersion": "2024-11-05", "capabilities": {} }
        }));
        let create = client.call(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "outl_page_create",
                "arguments": {
                    "slug": "frozen",
                    "title": "Frozen",
                    "content": [{ "text": "first" }]
                }
            }
        }));
        assert_eq!(create["result"]["isError"], false);
    }

    // Reproduce the exact state invariant 8 guards against: the `.md`
    // holds a line the op log never recorded, and its sidecar has
    // been re-stamped to declare those bytes faithful — the same
    // construction `outl-actions`'s own
    // `if_stale_refuses_when_the_md_carries_content_the_log_lacks`
    // uses. The hash gate alone cannot tell this apart from an
    // ordinary stale projection; only `content_lines_missing_from`
    // can, which is what the guarded write now consults.
    let md_path = ws.path().join("pages").join("frozen.md");
    let mut md = std::fs::read_to_string(&md_path).expect("read frozen.md");
    md.push_str("- only ever on disk\n");
    std::fs::write(&md_path, &md).expect("write unlogged line");

    let sidecar_path = outl_md::sidecar::sidecar_path_for(&md_path);
    let mut sidecar = outl_md::sidecar::read(&sidecar_path).expect("read sidecar");
    sidecar.last_synced_hash = outl_md::sidecar::file_hash(&md);
    outl_md::sidecar::write(&sidecar_path, &sidecar).expect("restamp sidecar");

    let mut client = McpClient::spawn(ws.path());
    let _ = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {} }
    }));

    let update = client.call(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "outl_page_update",
            "arguments": { "slug": "frozen", "title": "Frozen (renamed)" }
        }
    }));

    assert_eq!(
        update["result"]["isError"], true,
        "a frozen page's update must fail: {update}"
    );
    let structured = &update["result"]["structuredContent"];
    assert_eq!(structured["ok"], false);
    let error = &structured["error"];
    assert_eq!(
        error["code"], "PAGE_MARKDOWN_AHEAD_OF_LOG",
        "must be the distinct code, not a generic INTERNAL failure: {error}"
    );
    assert_eq!(error["data"]["lines"], 1);
    assert!(
        error["data"]["sample"]
            .as_str()
            .unwrap_or_default()
            .contains("only ever on disk"),
        "the sample must name the content at risk: {error}"
    );
    assert_eq!(
        error["data"]["recovery_command"],
        outl_actions::error::AHEAD_OF_LOG_RECOVERY_COMMAND
    );
    assert!(
        error["data"]["path"]
            .as_str()
            .unwrap_or_default()
            .ends_with("frozen.md"),
        "must name the file at risk: {error}"
    );

    // The whole point of the guard: the bytes on disk must survive
    // untouched, not get silently deleted by the refused write.
    let after = std::fs::read_to_string(&md_path).expect("re-read frozen.md");
    assert!(
        after.contains("only ever on disk"),
        "a refused write must never delete the unlogged content: {after:?}"
    );
}

/// `outl_block_prop_set` is the block-node counterpart of
/// `outl_page_prop_set` and shares its clear contract: an omitted or
/// null `value` removes the property, a present non-string must be
/// rejected rather than silently clearing it. Driving the real server
/// covers the dispatch branch, not just the `cmd/block.rs` handlers that
/// `tests/block_prop.rs` pins through the CLI.
#[test]
fn block_prop_set_over_mcp_clears_and_rejects_non_string() {
    let ws = init_workspace();
    let mut client = McpClient::spawn(ws.path());

    let _ = initialize(&mut client);

    let create = call_tool(
        &mut client,
        2,
        "outl_page_create",
        json!({ "slug": "tasks" }),
    );
    assert_eq!(create["result"]["isError"], false);

    let appended = call_tool(
        &mut client,
        3,
        "outl_block_append",
        json!({ "page": "tasks", "text": "TODO Fix the hyperdrive" }),
    );
    let id = success_data(&appended["result"])["id"]
        .as_str()
        .expect("append returns the new block id")
        .to_string();

    let set = call_tool(
        &mut client,
        4,
        "outl_block_prop_set",
        json!({ "id": id, "key": "due", "value": "2026-01-05" }),
    );
    assert_eq!(success_data(&set["result"])["value"], "2026-01-05");

    let get = call_tool(
        &mut client,
        5,
        "outl_block_prop_get",
        json!({ "id": id, "key": "due" }),
    );
    assert_eq!(success_data(&get["result"])["value"], "2026-01-05");

    // A null value clears it.
    let clear_null = call_tool(
        &mut client,
        6,
        "outl_block_prop_set",
        json!({ "id": id, "key": "due", "value": null }),
    );
    assert_eq!(success_data(&clear_null["result"])["value"], Value::Null);

    // Re-set, then clear by omitting the value entirely.
    let reset = call_tool(
        &mut client,
        7,
        "outl_block_prop_set",
        json!({ "id": id, "key": "due", "value": "2026-01-05" }),
    );
    assert_eq!(success_data(&reset["result"])["value"], "2026-01-05");

    let clear_omit = call_tool(
        &mut client,
        8,
        "outl_block_prop_set",
        json!({ "id": id, "key": "due" }),
    );
    assert_eq!(success_data(&clear_omit["result"])["value"], Value::Null);

    let get = call_tool(
        &mut client,
        9,
        "outl_block_prop_get",
        json!({ "id": id, "key": "due" }),
    );
    assert_eq!(get["result"]["isError"], true);
    assert_eq!(
        get["result"]["structuredContent"]["error"]["code"],
        "PROP_NOT_FOUND"
    );

    // A non-string value is rejected, not silently cleared.
    let bad = call_tool(
        &mut client,
        10,
        "outl_block_prop_set",
        json!({ "id": id, "key": "due", "value": 42 }),
    );
    assert_eq!(bad["result"]["isError"], true);
    assert_eq!(
        bad["result"]["structuredContent"]["error"]["code"],
        "INVALID_ARG"
    );
}

/// The date filters (`before:` / `after:`) are only as good as the
/// `due::` date already on the block, so this drives the whole loop an
/// agent runs over MCP: stamp a date with `outl_block_prop_set` (and, to
/// cover the batch op, with `outl_batch`), read it back through
/// `outl_query_dsl`, then clear it and watch the row drop out.
///
/// Absolute ISO thresholds are used rather than `+7d` relative sugar so
/// the assertion does not depend on the machine clock. After any write the
/// cached index is invalidated (`MUTATING`), which is what lets a
/// follow-up `outl_query_dsl` in the same session see the new date.
#[test]
fn date_filter_over_mcp_reads_block_prop_set_and_batch() {
    let ws = init_workspace();
    let mut client = McpClient::spawn(ws.path());

    let _ = initialize(&mut client);
    let _ = call_tool(
        &mut client,
        2,
        "outl_page_create",
        json!({ "slug": "tasks" }),
    );

    let soon = call_tool(
        &mut client,
        3,
        "outl_block_append",
        json!({ "page": "tasks", "text": "TODO Ship the release" }),
    );
    let soon_id = success_data(&soon["result"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    let later = call_tool(
        &mut client,
        4,
        "outl_block_append",
        json!({ "page": "tasks", "text": "TODO Write the memo" }),
    );
    let later_id = success_data(&later["result"])["id"]
        .as_str()
        .unwrap()
        .to_string();

    // One date via the dedicated tool, the other via the batch op.
    let _ = call_tool(
        &mut client,
        5,
        "outl_block_prop_set",
        json!({ "id": soon_id, "key": "due", "value": "2026-01-05" }),
    );
    let batch = call_tool(
        &mut client,
        6,
        "outl_batch",
        json!({
            "ops": [
                { "op": "block_prop_set",
                  "args": { "id": later_id, "key": "due", "value": "2026-12-25" } }
            ]
        }),
    );
    assert_eq!(
        success_data(&batch["result"])["results"][0]["op"],
        "block_prop_set"
    );

    // Only the 2026-01-05 task is due before the threshold.
    let hits = |client: &mut McpClient, id: i64, dsl: &str| -> Vec<Value> {
        let r = call_tool(client, id, "outl_query_dsl", json!({ "dsl": dsl }));
        success_data(&r["result"])["hits"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    };

    let due_soon = hits(&mut client, 7, "status: todo\nbefore: due 2026-06-01");
    assert_eq!(
        due_soon.len(),
        1,
        "only the January task qualifies: {due_soon:?}"
    );
    assert_eq!(due_soon[0]["page"], "tasks");
    assert_eq!(due_soon[0]["text"], "Ship the release");

    let due_later = hits(&mut client, 8, "status: todo\nafter: due 2026-06-01");
    assert_eq!(
        due_later.len(),
        1,
        "only the December task qualifies: {due_later:?}"
    );
    assert_eq!(due_later[0]["text"], "Write the memo");

    // Clearing the soon date drops it from the date query (index refreshed
    // in-session), proving the write path and the read path agree.
    let _ = call_tool(
        &mut client,
        9,
        "outl_block_prop_set",
        json!({ "id": soon_id, "key": "due", "value": null }),
    );
    let cleared = hits(&mut client, 10, "status: todo\nbefore: due 2026-06-01");
    assert!(
        cleared.is_empty(),
        "clearing the date must remove the row from the date filter: {cleared:?}"
    );
}

/// A date query that finds a block is only useful to an agent if the row
/// carries an id the agent can act on. The ref handle (`blk-XXXXXX`) is
/// display-only — every block write tool is ULID-only — so a hit that
/// returned *only* the handle could be read but never written back (the
/// gap this pins closed). This asserts the writable `id` is present and
/// round-trips into `outl_block_prop_set`, and that the `block` handle is
/// *not* accepted, so removing the `id` field fails here rather than
/// shipping an agent that can find a task by date but not act on it.
#[test]
fn query_dsl_hits_carry_an_id_a_write_tool_accepts() {
    let ws = init_workspace();
    let mut client = McpClient::spawn(ws.path());

    let _ = initialize(&mut client);
    let _ = call_tool(
        &mut client,
        2,
        "outl_page_create",
        json!({ "slug": "tasks" }),
    );

    let appended = call_tool(
        &mut client,
        3,
        "outl_block_append",
        json!({ "page": "tasks", "text": "TODO Ship the release" }),
    );
    let id = success_data(&appended["result"])["id"]
        .as_str()
        .expect("append returns the new block id")
        .to_string();

    let _ = call_tool(
        &mut client,
        4,
        "outl_block_prop_set",
        json!({ "id": id, "key": "due", "value": "2026-01-05" }),
    );

    let r = call_tool(
        &mut client,
        5,
        "outl_query_dsl",
        json!({ "dsl": "status: todo\nbefore: due 2026-06-01" }),
    );
    let hit = success_data(&r["result"])["hits"][0].clone();

    // The hit carries the ULID a write tool accepts — the same id append
    // handed back, and the handle is a separate, display-only field.
    assert_eq!(hit["id"], id, "hit must carry the actionable ULID");
    let handle = hit["block"].as_str().expect("hit carries a ref handle");
    assert!(
        handle.starts_with("blk-"),
        "handle is display-only: {handle}"
    );

    // The id round-trips straight into the write tool — the agent loop.
    let rewrite = call_tool(
        &mut client,
        6,
        "outl_block_prop_set",
        json!({ "id": hit["id"], "key": "due", "value": "2026-02-02" }),
    );
    assert_eq!(
        success_data(&rewrite["result"])["value"],
        "2026-02-02",
        "a query id must be accepted by outl_block_prop_set"
    );

    // ...and the handle alone is *not*, which is why the id is load-bearing.
    let from_handle = call_tool(
        &mut client,
        7,
        "outl_block_prop_set",
        json!({ "id": handle, "key": "due", "value": "2026-03-03" }),
    );
    assert_eq!(from_handle["result"]["isError"], true);
    assert_eq!(
        from_handle["result"]["structuredContent"]["error"]["code"],
        "INVALID_BLOCK_ID"
    );
}

/// `initialize` handshake shared by the newer tests.
fn initialize(client: &mut McpClient) -> Value {
    client.call(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {} }
    }))
}

/// Build and send a `tools/call`, returning the raw JSON-RPC response.
fn call_tool(client: &mut McpClient, id: i64, name: &str, arguments: Value) -> Value {
    client.call(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": name, "arguments": arguments }
    }))
}
