//! `--demo`: simulated knights and apprentices, no herdr needed.

use std::sync::mpsc::Sender;
use std::time::Duration;

use crate::herdr::{AgentInfo, Snapshot};
use crate::hooks::{HookEvent, HookKind};
use crate::model::{Rng, Workspace};
use crate::Msg;

const TICK: f32 = 0.25;
const TASKS: [&str; 10] = [
    "explore auth flow",
    "review the diff",
    "find hook docs",
    "write tests",
    "map socket api",
    "check lint",
    "plan schema",
    "read changelog",
    "trace a bug",
    "bench render",
];
const TITLES: [&str; 6] = ["Read src/main.rs", "Edit src/pane.rs", "Bash cargo test", "Grep \"subagent\"", "認証フローの調査", "Write README.md"];

struct Fake {
    pane: String,
    ws: usize,
    name: &'static str,
    state: &'static str,
    t: f32,
    title: &'static str,
    subs: Vec<(String, f32)>,
}

pub fn spawn(tx: Sender<Msg>, knights: usize) {
    std::thread::spawn(move || {
        let mut rng = Rng::new(42);
        let ws_names = ["core", "apps", "infra", "docs"];
        let names = [
            "claude:api", "codex:web", "claude:docs", "claude:infra", "codex:mobile", "claude:auth", "pi:scripts",
            "claude:tests", "codex:ui", "claude:db", "codex:cli", "claude:search", "pi:ops", "claude:perf", "codex:etl",
            "claude:ml", "codex:sdk", "claude:sec",
        ];
        let states = ["working", "working", "idle", "working", "blocked", "working", "done"];
        let mut fakes: Vec<Fake> = (0..knights)
            .map(|i| Fake {
                pane: format!("w{}:p{}", i / 5 + 1, i % 5 + 1),
                ws: i / 5,
                name: names[i % names.len()],
                state: states[i % states.len()],
                t: 0.0,
                title: TITLES[i % TITLES.len()],
                subs: Vec::new(),
            })
            .collect();
        let mut seq = 0;
        let send_hook = |tx: &Sender<Msg>, kind, pane: &str, id: Option<String>, task: Option<&str>| {
            let ev = HookEvent { kind, pane: pane.into(), session: None, agent_id: id, agent_type: None, task: task.map(Into::into), ts: 0.0 };
            tx.send(Msg::Hook(ev)).is_ok()
        };
        loop {
            let snap = Snapshot {
                agents: fakes
                    .iter()
                    .map(|f| AgentInfo {
                        pane_id: f.pane.clone(),
                        workspace_id: format!("w{}", f.ws + 1),
                        tab_id: format!("w{}:t1", f.ws + 1),
                        agent: None,
                        label: Some(f.name.into()),
                        status: f.state.into(),
                        title: f.title.into(),
                    })
                    .collect(),
                workspaces: (0..knights.div_ceil(5))
                    .map(|i| Workspace { id: format!("w{}", i + 1), label: ws_names[i % ws_names.len()].into() })
                    .collect(),
            };
            if tx.send(Msg::Snapshot(snap)).is_err() {
                return;
            }
            for f in fakes.iter_mut() {
                f.t += TICK;
                for s in f.subs.iter_mut() {
                    s.1 -= TICK;
                }
                let done: Vec<String> = f.subs.iter().filter(|s| s.1 <= 0.0).map(|s| s.0.clone()).collect();
                f.subs.retain(|s| s.1 > 0.0);
                for id in done {
                    send_hook(&tx, HookKind::Stop, &f.pane, Some(id), None);
                }
                match f.state {
                    "working" => {
                        if rng.f() < 0.25 {
                            f.title = rng.pick(&TITLES);
                        }
                        if f.subs.len() < 6 && rng.f() < TICK * 0.12 {
                            seq += 1;
                            let id = format!("demo{seq}");
                            let n = if rng.f() < 0.15 { 4 } else { 1 };
                            for k in 0..n {
                                let id = format!("{id}-{k}");
                                send_hook(&tx, HookKind::Task, &f.pane, None, Some(rng.pick(&TASKS)));
                                send_hook(&tx, HookKind::Start, &f.pane, Some(id.clone()), None);
                                f.subs.push((id, rng.range(7.0, 16.0)));
                            }
                        }
                        if rng.f() < TICK * 0.012 {
                            f.state = "blocked";
                            f.t = 0.0;
                        } else if f.t > 28.0 && f.subs.is_empty() {
                            f.state = "done";
                            f.t = 0.0;
                        }
                    }
                    "blocked" if f.t > 8.0 => (f.state, f.t) = ("working", 0.0),
                    "done" if f.t > 6.0 => (f.state, f.t) = ("idle", 0.0),
                    "idle" if f.t > 7.0 => (f.state, f.t) = ("working", 0.0),
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_secs_f32(TICK));
        }
    });
}
