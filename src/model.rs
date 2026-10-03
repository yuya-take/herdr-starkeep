//! Knights (herdr agents), apprentices (subagents) and floors (workspaces).
//!
//! The model advances regardless of what is on screen; only positions and paths
//! are specific to the large view.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::herdr::Snapshot;
use crate::hooks::{HookEvent, HookKind};
use crate::render::Rgb;
use crate::spots;
use crate::sprites::{CRYSTALS, HAIRS, SKINS};

/// An apprentice whose tool call never turned into a subagent (denied, failed).
const UNCLAIMED_TTL: Duration = Duration::from_secs(90);
/// Upper bound for a subagent whose stop event got lost.
const STALE_TTL: Duration = Duration::from_secs(3 * 60 * 60);
/// Hook events for panes herdr has not reported yet are kept this long.
const PENDING_TTL: Duration = Duration::from_secs(10);
/// Most apprentices drawn per knight before we stop admitting more.
pub const MAX_APPRENTICES: usize = 14;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KnightState {
    Working,
    Blocked,
    Done,
    Idle,
    Unknown,
}

impl KnightState {
    pub fn parse(s: &str) -> KnightState {
        match s {
            "working" => KnightState::Working,
            "blocked" => KnightState::Blocked,
            "done" => KnightState::Done,
            "idle" => KnightState::Idle,
            _ => KnightState::Unknown,
        }
    }
    pub fn glyph(self) -> &'static str {
        match self {
            KnightState::Working => ">",
            KnightState::Blocked => "!",
            KnightState::Done => "*",
            KnightState::Idle => "~",
            KnightState::Unknown => "?",
        }
    }
}

pub struct Knight {
    pub pane_id: String,
    pub workspace_id: String,
    pub name: String,
    pub color_idx: usize,
    pub crystal: Rgb,
    pub skin: Rgb,
    pub state: KnightState,
    /// What herdr shows as the pane title; the closest thing to "current task".
    pub title: String,
    /// Crystal flashes after an apprentice hands over its cube.
    pub flash: f32,
    /// Bay index while the knight is shown in the large view.
    pub bay: Option<usize>,
    sort_key: (usize, u32, u32),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Spot {
    Seat(u8),
    Floor(u8),
    Offdeck,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Enter,
    Work,
    Move,
    Report,
    Bow,
    Exit,
}

pub struct Apprentice {
    pub id: u64,
    pub agent_id: Option<String>,
    /// Claude Code session that summoned it.
    pub session: Option<String>,
    pub master: String,
    pub task: String,
    pub spot: Spot,
    pub phase: Phase,
    pub x: f32,
    pub y: f32,
    pub path: VecDeque<(f32, f32)>,
    pub back: bool,
    pub seated: bool,
    pub skin: Rgb,
    pub hair: Rgb,
    pub walk_t: f32,
    pub bow_t: f32,
    pub delivered: bool,
    pub born: Instant,
}

impl Apprentice {
    /// Still doing the job (as opposed to reporting or leaving).
    pub fn active(&self) -> bool {
        matches!(self.phase, Phase::Enter | Phase::Work | Phase::Move)
    }
}

pub struct Workspace {
    pub id: String,
    pub label: String,
}

pub struct Floor {
    pub workspace_id: String,
    pub name: String,
    /// pane ids, left to right
    pub knights: Vec<String>,
}

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed | 1)
    }
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn f(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + self.f() * (b - a)
    }
    pub fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[(self.next() % xs.len() as u64) as usize]
    }
}

pub struct World {
    pub knights: Vec<Knight>,
    pub workspaces: Vec<Workspace>,
    pub apps: Vec<Apprentice>,
    pub time: f32,
    pub online: bool,
    pub status: String,
    /// Right edge of the large scene; apprentices enter and leave past it.
    pub scene_w: i32,
    pub rng: Rng,
    next_app: u64,
    pending: Vec<(Instant, HookEvent, bool)>,
    /// True once any SubagentStart arrived, so unclaimed tool calls can expire.
    saw_start: bool,
}

impl World {
    pub fn new() -> World {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(7);
        World {
            knights: Vec::new(),
            workspaces: Vec::new(),
            apps: Vec::new(),
            time: 0.0,
            online: false,
            status: "connecting to herdr".into(),
            scene_w: 112,
            rng: Rng::new(seed),
            next_app: 1,
            pending: Vec::new(),
            saw_start: false,
        }
    }

    pub fn knight(&self, pane: &str) -> Option<&Knight> {
        self.knights.iter().find(|k| k.pane_id == pane)
    }
    pub fn knight_mut(&mut self, pane: &str) -> Option<&mut Knight> {
        self.knights.iter_mut().find(|k| k.pane_id == pane)
    }
    pub fn workspace_label<'a>(&'a self, id: &'a str) -> &'a str {
        self.workspaces
            .iter()
            .find(|w| w.id == id)
            .map(|w| w.label.as_str())
            .unwrap_or(id)
    }
    pub fn crew(&self, pane: &str) -> impl Iterator<Item = &Apprentice> {
        let pane = pane.to_string();
        self.apps.iter().filter(move |p| p.master == pane)
    }
    pub fn active_count(&self, pane: &str) -> usize {
        self.crew(pane).filter(|p| p.active()).count()
    }

    pub fn apply_snapshot(&mut self, snap: Snapshot) {
        self.online = true;
        self.status.clear();
        self.workspaces = snap.workspaces;
        let alive: Vec<&str> = snap.agents.iter().map(|a| a.pane_id.as_str()).collect();
        self.knights.retain(|k| alive.contains(&k.pane_id.as_str()));
        self.apps.retain(|p| alive.contains(&p.master.as_str()));

        for a in &snap.agents {
            let ws_index = self
                .workspaces
                .iter()
                .position(|w| w.id == a.workspace_id)
                .unwrap_or(usize::MAX);
            let ws_label = self.workspace_label(&a.workspace_id).to_string();
            let name = match a.label.as_deref() {
                Some(l) if !l.is_empty() => l.to_string(),
                _ => format!("{}:{}", a.agent.as_deref().unwrap_or("agent"), ws_label),
            };
            let sort_key = (ws_index, id_number(&a.tab_id), id_number(&a.pane_id));
            let state = KnightState::parse(&a.status);
            if let Some(k) = self.knight_mut(&a.pane_id) {
                k.state = state;
                k.name = name;
                k.title = a.title.clone();
                k.workspace_id = a.workspace_id.clone();
                k.sort_key = sort_key;
                continue;
            }
            let used: Vec<usize> = self.knights.iter().map(|k| k.color_idx).collect();
            let color_idx = (0..).find(|i| !used.contains(i)).unwrap_or(0);
            self.knights.push(Knight {
                pane_id: a.pane_id.clone(),
                workspace_id: a.workspace_id.clone(),
                name,
                color_idx,
                crystal: CRYSTALS[color_idx % CRYSTALS.len()],
                skin: SKINS[(color_idx * 3 + 1) % SKINS.len()],
                state,
                title: a.title.clone(),
                flash: 0.0,
                bay: None,
                sort_key,
            });
        }
        self.knights.sort_by_key(|k| k.sort_key);

        let pending = std::mem::take(&mut self.pending);
        for (at, ev, replay) in pending {
            if at.elapsed() < PENDING_TTL {
                self.hook(ev, replay);
            }
        }
    }

    pub fn set_offline(&mut self, why: String) {
        self.online = false;
        self.status = why;
    }

    /// Floors for the overview: one per workspace, split when it has more
    /// knights than desks.
    pub fn floors(&self, per_floor: usize) -> Vec<Floor> {
        let mut floors: Vec<Floor> = Vec::new();
        let mut i = 0;
        while i < self.knights.len() {
            let ws = &self.knights[i].workspace_id;
            let run: Vec<String> = self.knights[i..]
                .iter()
                .take_while(|k| &k.workspace_id == ws)
                .map(|k| k.pane_id.clone())
                .collect();
            i += run.len();
            let label = self.workspace_label(ws);
            let parts = run.chunks(per_floor.max(1)).count();
            for (n, chunk) in run.chunks(per_floor.max(1)).enumerate() {
                let name = if parts > 1 {
                    format!("{} {}/{}", label, n + 1, parts)
                } else {
                    label.to_string()
                };
                floors.push(Floor {
                    workspace_id: ws.clone(),
                    name,
                    knights: chunk.to_vec(),
                });
            }
        }
        floors
    }

    // ---------- apprentices ----------

    pub fn hook(&mut self, ev: HookEvent, replay: bool) {
        if self.knight(&ev.pane).is_none() {
            // herdr may not have reported this pane yet (or replay ran first).
            if replay || matches!(ev.kind, HookKind::Task | HookKind::Start) {
                self.pending.push((Instant::now(), ev, replay));
            }
            return;
        }
        match ev.kind {
            HookKind::Task => {
                let task = ev.task.or(ev.agent_type).unwrap_or_else(|| "subagent".into());
                self.summon(&ev.pane, task, None, ev.session, replay);
            }
            HookKind::Start => {
                self.saw_start = true;
                let Some(id) = ev.agent_id else { return };
                if self.apps.iter().any(|p| p.agent_id.as_deref() == Some(id.as_str())) {
                    return;
                }
                let claim = self
                    .apps
                    .iter_mut()
                    .filter(|p| p.master == ev.pane && p.active() && p.agent_id.is_none() && p.session == ev.session)
                    .min_by_key(|p| p.id);
                match claim {
                    Some(p) => p.agent_id = Some(id),
                    None => {
                        let task = ev.task.or(ev.agent_type).unwrap_or_else(|| "subagent".into());
                        let session = ev.session.clone();
                        self.summon(&ev.pane, task, Some(id), session, replay);
                    }
                }
            }
            HookKind::Stop => {
                let by_id = ev.agent_id.as_deref().and_then(|id| {
                    self.apps
                        .iter()
                        .find(|p| p.active() && p.agent_id.as_deref() == Some(id))
                        .map(|p| p.id)
                });
                let target = by_id.or_else(|| {
                    self.apps
                        .iter()
                        .filter(|p| p.master == ev.pane && p.active() && p.session == ev.session)
                        .map(|p| p.id)
                        .min()
                });
                if let Some(id) = target {
                    if replay {
                        self.apps.retain(|p| p.id != id);
                    } else {
                        spots::finish_work(self, id);
                    }
                }
            }
            HookKind::End => {
                self.apps.retain(|p| !(p.master == ev.pane && p.session == ev.session));
            }
        }
    }

    pub fn summon(
        &mut self,
        master: &str,
        task: String,
        agent_id: Option<String>,
        session: Option<String>,
        replay: bool,
    ) -> bool {
        if self.active_count(master) >= MAX_APPRENTICES {
            return false;
        }
        let spot = spots::assign_spot(&self.apps, master);
        let id = self.next_app;
        self.next_app += 1;
        let skin = self.rng.pick(&SKINS);
        let hair = self.rng.pick(&HAIRS);
        let mut p = Apprentice {
            id,
            agent_id,
            session,
            master: master.to_string(),
            task,
            spot,
            phase: if spot == Spot::Offdeck {
                Phase::Work
            } else {
                Phase::Enter
            },
            x: (self.scene_w + 1) as f32,
            y: spots::LANE,
            path: VecDeque::new(),
            back: false,
            seated: false,
            skin,
            hair,
            walk_t: 0.0,
            bow_t: 0.0,
            delivered: false,
            born: Instant::now(),
        };
        if spot != Spot::Offdeck {
            let bay = self.knight(master).and_then(|k| k.bay).unwrap_or(0);
            spots::route_to(&mut p, bay, spot);
        }
        if replay {
            spots::place(&mut p, self.knight(master).and_then(|k| k.bay).unwrap_or(0));
        }
        self.apps.push(p);
        true
    }

    /// Drop apprentices whose stop event never arrived.
    pub fn sweep(&mut self) {
        let saw_start = self.saw_start;
        self.apps.retain(|p| {
            if !p.active() {
                return true;
            }
            let unclaimed = saw_start && p.agent_id.is_none() && p.born.elapsed() > UNCLAIMED_TTL;
            !(unclaimed || p.born.elapsed() > STALE_TTL)
        });
    }
}

/// Numeric part of an id like `w2:p10` (-> 10) or `w2:t1` (-> 1).
fn id_number(id: &str) -> u32 {
    id.rsplit(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::herdr::AgentInfo;

    fn agent(pane: &str, ws: &str, status: &str) -> AgentInfo {
        AgentInfo {
            pane_id: pane.into(),
            workspace_id: ws.into(),
            tab_id: format!("{ws}:t1"),
            agent: Some("claude".into()),
            label: None,
            status: status.into(),
            title: "title".into(),
        }
    }

    fn world(agents: Vec<AgentInfo>) -> World {
        let mut w = World::new();
        let workspaces = vec![
            Workspace {
                id: "w1".into(),
                label: "core".into(),
            },
            Workspace {
                id: "w2".into(),
                label: "apps".into(),
            },
        ];
        w.apply_snapshot(Snapshot { agents, workspaces });
        w
    }

    fn ev(kind: HookKind, pane: &str, id: Option<&str>, task: Option<&str>) -> HookEvent {
        HookEvent {
            kind,
            pane: pane.into(),
            session: Some("s".into()),
            agent_id: id.map(Into::into),
            agent_type: None,
            task: task.map(Into::into),
            ts: 0.0,
        }
    }

    #[test]
    fn snapshot_orders_and_names_knights() {
        let w = world(vec![
            agent("w2:p1", "w2", "idle"),
            agent("w1:p10", "w1", "working"),
            agent("w1:p2", "w1", "blocked"),
        ]);
        let panes: Vec<&str> = w.knights.iter().map(|k| k.pane_id.as_str()).collect();
        assert_eq!(panes, ["w1:p2", "w1:p10", "w2:p1"]);
        assert_eq!(w.knights[0].name, "claude:core");
        assert_eq!(w.knights[0].state, KnightState::Blocked);
        let floors = w.floors(1);
        assert_eq!(floors.len(), 3);
        assert_eq!(floors[0].name, "core 1/2");
    }

    #[test]
    fn spots_fill_seats_then_floor_then_offdeck_and_promote() {
        let mut w = world(vec![agent("w1:p1", "w1", "working")]);
        for i in 0..7 {
            w.hook(ev(HookKind::Task, "w1:p1", None, Some(&format!("t{i}"))), false);
        }
        let spots: Vec<Spot> = w.apps.iter().map(|p| p.spot).collect();
        assert_eq!(
            spots,
            [
                Spot::Seat(0),
                Spot::Seat(1),
                Spot::Floor(0),
                Spot::Floor(1),
                Spot::Floor(2),
                Spot::Offdeck,
                Spot::Offdeck
            ]
        );
        // Claim ids in order, then stop the first seat.
        for i in 0..7 {
            w.hook(ev(HookKind::Start, "w1:p1", Some(&format!("a{i}")), None), false);
        }
        assert_eq!(w.apps[0].agent_id.as_deref(), Some("a0"));
        w.hook(ev(HookKind::Stop, "w1:p1", Some("a0"), None), false);
        assert_eq!(w.apps[0].phase, Phase::Report);
        for p in w.apps.iter_mut() {
            if p.phase == Phase::Enter {
                p.phase = Phase::Work;
            }
        }
        spots::promote_all(&mut w);
        let by_task = |t: &str| w.apps.iter().find(|p| p.task == t).unwrap().spot;
        assert_eq!(by_task("t2"), Spot::Seat(0));
        assert_eq!(by_task("t5"), Spot::Floor(0));
        assert_eq!(by_task("t6"), Spot::Offdeck);
    }

    #[test]
    fn hooks_for_unknown_pane_wait_for_snapshot() {
        let mut w = world(vec![]);
        w.hook(ev(HookKind::Task, "w1:p1", None, Some("x")), false);
        assert!(w.apps.is_empty());
        w.apply_snapshot(Snapshot {
            agents: vec![agent("w1:p1", "w1", "working")],
            workspaces: vec![Workspace {
                id: "w1".into(),
                label: "core".into(),
            }],
        });
        assert_eq!(w.apps.len(), 1);
    }

    #[test]
    fn replayed_stop_removes_without_ceremony() {
        let mut w = world(vec![agent("w1:p1", "w1", "working")]);
        w.hook(ev(HookKind::Task, "w1:p1", None, Some("x")), true);
        w.hook(ev(HookKind::Start, "w1:p1", Some("a"), None), true);
        w.hook(ev(HookKind::Stop, "w1:p1", Some("a"), None), true);
        assert!(w.apps.is_empty());
    }

    #[test]
    fn session_end_only_dismisses_its_own_apprentices() {
        let mut w = world(vec![agent("w1:p1", "w1", "working")]);
        w.hook(ev(HookKind::Task, "w1:p1", None, Some("mine")), false);
        let mut other = ev(HookKind::End, "w1:p1", None, None);
        other.session = Some("nested".into());
        w.hook(other, false);
        assert_eq!(w.apps.len(), 1);
        w.hook(ev(HookKind::End, "w1:p1", None, None), false);
        assert!(w.apps.is_empty());
    }
}
