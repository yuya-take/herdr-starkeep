//! Apprentice events written by the Claude Code hook (`hooks/hook.sh`), one
//! JSON object per line.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

use serde::Deserialize;

use crate::Msg;

/// Events older than this are ignored when replaying the file at startup.
const REPLAY_WINDOW_SECS: f64 = 3.0 * 60.0 * 60.0;
/// The file is trimmed at startup once it grows past this.
const MAX_BYTES: u64 = 2 * 1024 * 1024;
const KEEP_LINES: usize = 2000;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HookKind {
    /// PreToolUse of the Agent/Task tool: a subagent is being summoned.
    Task,
    /// SubagentStart: carries the subagent's id.
    Start,
    /// SubagentStop
    Stop,
    /// SessionEnd: the knight's session is gone, dismiss everyone.
    End,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HookEvent {
    #[serde(rename = "ev")]
    pub kind: HookKind,
    pub pane: String,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub ts: f64,
}

pub fn events_path() -> PathBuf {
    if let Some(p) = std::env::var_os("STARKEEP_EVENTS") {
        return p.into();
    }
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    state.join("starkeep/events.jsonl")
}

pub fn parse(line: &str) -> Option<HookEvent> {
    let ev: HookEvent = serde_json::from_str(line.trim()).ok()?;
    (!ev.pane.is_empty()).then_some(ev)
}

fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn trim(path: &PathBuf) {
    let Ok(meta) = std::fs::metadata(path) else { return };
    if meta.len() <= MAX_BYTES {
        return;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let lines: Vec<&str> = text.lines().collect();
    let keep = lines[lines.len().saturating_sub(KEEP_LINES)..].join("\n") + "\n";
    let _ = std::fs::write(path, keep);
}

/// Replay recent events, then follow the file.
pub fn spawn_tail(tx: Sender<Msg>) {
    let path = events_path();
    std::thread::spawn(move || {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        trim(&path);
        let mut offset = 0u64;
        if let Ok(f) = File::open(&path) {
            let cutoff = now_secs() - REPLAY_WINDOW_SECS;
            let mut events = Vec::new();
            let mut reader = BufReader::new(f);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                if line.ends_with('\n') {
                    offset += line.len() as u64;
                    if let Some(ev) = parse(&line).filter(|e| e.ts >= cutoff) {
                        events.push(ev);
                    }
                }
                line.clear();
            }
            if tx.send(Msg::Replay(events)).is_err() {
                return;
            }
        }
        let mut partial = String::new();
        loop {
            std::thread::sleep(Duration::from_millis(200));
            let Ok(mut f) = File::open(&path) else { continue };
            let len = f.metadata().map(|m| m.len()).unwrap_or(0);
            if len < offset {
                offset = 0; // truncated or replaced
                partial.clear();
            }
            if len == offset || f.seek(SeekFrom::Start(offset)).is_err() {
                continue;
            }
            let mut chunk = String::new();
            let Ok(n) = f.read_to_string(&mut chunk) else { continue };
            offset += n as u64;
            partial.push_str(&chunk);
            while let Some(i) = partial.find('\n') {
                let line: String = partial.drain(..=i).collect();
                if let Some(ev) = parse(&line) {
                    if tx.send(Msg::Hook(ev)).is_err() {
                        return;
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hook_lines() {
        let ev = parse(r#"{"ev":"task","pane":"w1:p1","session":"s","agent_id":null,"agent_type":"Explore","task":"map socket api","ts":1.5}"#).unwrap();
        assert_eq!(ev.kind, HookKind::Task);
        assert_eq!(ev.task.as_deref(), Some("map socket api"));
        assert!(parse(r#"{"ev":"stop","pane":"","ts":1}"#).is_none());
        assert!(parse(r#"{"ev":"other","pane":"w1:p1"}"#).is_none());
    }
}
