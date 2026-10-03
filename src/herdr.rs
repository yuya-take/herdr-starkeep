//! Knight state from the herdr socket (newline-delimited JSON).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

use serde_json::{json, Value};

use crate::model::Workspace;
use crate::Msg;

const POLL: Duration = Duration::from_millis(500);

pub struct AgentInfo {
    pub pane_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub agent: Option<String>,
    pub label: Option<String>,
    pub status: String,
    pub title: String,
}

pub struct Snapshot {
    pub agents: Vec<AgentInfo>,
    pub workspaces: Vec<Workspace>,
}

pub fn socket_path() -> PathBuf {
    if let Some(p) = std::env::var_os("HERDR_SOCKET_PATH") {
        return p.into();
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join(".config/herdr/herdr.sock")
}

/// herdr answers one request per connection, so each call connects anew.
pub fn call(method: &str, params: Value) -> Result<Value, String> {
    let stream = UnixStream::connect(socket_path()).map_err(|e| format!("cannot connect ({e})"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
    let req = json!({ "id": "starkeep", "method": method, "params": params });
    writeln!(writer, "{req}").map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&line).map_err(|e| format!("bad response ({e})"))?;
    if let Some(err) = v.get("error") {
        return Err(err["message"].as_str().unwrap_or("error").to_string());
    }
    Ok(v["result"].clone())
}

pub fn snapshot() -> Result<Snapshot, String> {
    let agents = call("agent.list", json!({}))?;
    let workspaces = call("workspace.list", json!({}))?;
    Ok(parse_snapshot(&agents, &workspaces))
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

pub fn parse_snapshot(agents: &Value, workspaces: &Value) -> Snapshot {
    let agents = agents["agents"]
        .as_array()
        .map(|xs| {
            xs.iter()
                .filter_map(|a| {
                    Some(AgentInfo {
                        pane_id: s(a, "pane_id")?,
                        workspace_id: s(a, "workspace_id")?,
                        tab_id: s(a, "tab_id").unwrap_or_default(),
                        agent: s(a, "display_agent").or_else(|| s(a, "agent")),
                        label: s(a, "label").or_else(|| s(a, "name")),
                        status: s(a, "agent_status").unwrap_or_else(|| "unknown".into()),
                        title: s(a, "terminal_title_stripped")
                            .or_else(|| s(a, "title"))
                            .unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let workspaces = workspaces["workspaces"]
        .as_array()
        .map(|xs| {
            xs.iter()
                .filter_map(|w| {
                    let id = s(w, "workspace_id")?;
                    let label = s(w, "label").unwrap_or_else(|| id.clone());
                    Some(Workspace { id, label })
                })
                .collect()
        })
        .unwrap_or_default();
    Snapshot { agents, workspaces }
}

/// Poll herdr forever.
pub fn spawn_poller(tx: Sender<Msg>) {
    std::thread::spawn(move || loop {
        let msg = match snapshot() {
            Ok(snap) => Msg::Snapshot(snap),
            Err(e) => Msg::Offline(e),
        };
        if tx.send(msg).is_err() {
            return;
        }
        std::thread::sleep(POLL);
    });
}

/// Focus the pane hosting an agent.
pub fn focus_agent(pane: &str) -> Result<(), String> {
    call("agent.focus", json!({ "target": pane })).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_agent_list() {
        let agents = json!({"agents":[{"agent":"claude","agent_status":"working","pane_id":"w9:p1","tab_id":"w9:t1",
            "terminal_title_stripped":"Artifact","workspace_id":"w9"}],"type":"agent_list"});
        let ws = json!({"type":"workspace_list","workspaces":[{"label":"herdr-starkeep","workspace_id":"w9"}]});
        let snap = parse_snapshot(&agents, &ws);
        assert_eq!(snap.agents.len(), 1);
        assert_eq!(snap.agents[0].status, "working");
        assert_eq!(snap.agents[0].title, "Artifact");
        assert_eq!(snap.workspaces[0].label, "herdr-starkeep");
    }
}
