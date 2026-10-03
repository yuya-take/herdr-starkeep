//! The two zoom levels: the cross-section of every floor, and a close-up of
//! three bays. Picks which one to show and draws it.

use crate::anim::{self, Starfield};
use crate::model::{Apprentice, Floor, Knight, KnightState, Phase, Spot, World};
use crate::render::{fit, Canvas, Rgb, SpriteOpts};
use crate::spots::{station, BAY_W};
use crate::sprites::{self, *};

/// Columns one desk takes in the overview.
pub const DESK_W: i32 = 22;
/// Rows one floor takes in the overview.
const FLOOR_H: i32 = 10;

const HB: Rgb = Rgb::hex(0x0c1020);
const HEAD_FG: Rgb = Rgb::hex(0x9fb3e0);
const MUTED: Rgb = Rgb::hex(0x6f7fa8);
const AMBER: Rgb = Rgb::hex(0xffd166);
const AMBER_DIM: Rgb = Rgb::hex(0xb8892c);
const AMBER_INK: Rgb = Rgb::hex(0x1a1200);
const BUBBLE: Rgb = Rgb::hex(0xeef1f8);
const RED: Rgb = Rgb::hex(0xe5484d);
const GREEN: Rgb = Rgb::hex(0x2fae5b);
const SPACE: Rgb = Rgb::hex(0x05080f);
const WALL: Rgb = Rgb::hex(0x1a2036);
const FRAME: Rgb = Rgb::hex(0x2e3858);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Overview,
    Large { focus: bool },
}

pub struct Layout {
    pub mode: Mode,
    /// pane ids shown in the large view, one per bay
    pub shown: Vec<String>,
    pub floor_ws: Option<String>,
    pub start: usize,
    pub total: usize,
    pub cols: u16,
    pub rows: u16,
    pub per_floor: usize,
    pub bays: usize,
    pub floors_visible: usize,
    /// Left offset that centres the large scene.
    pub ox: i32,
}

pub struct View {
    pub focus: Option<String>,
    pub cursor: Option<String>,
    pub kb_used: bool,
    pub scroll: usize,
    pub layout: Layout,
    pub notice: Option<(String, f32)>,
    layout_key: String,
    big_stars: Option<Starfield>,
    floor_stars: Vec<Starfield>,
}

impl View {
    pub fn new() -> View {
        View {
            focus: None,
            cursor: None,
            kb_used: false,
            scroll: 0,
            layout: Layout {
                mode: Mode::Overview,
                shown: Vec::new(),
                floor_ws: None,
                start: 0,
                total: 0,
                cols: 112,
                rows: 34,
                per_floor: 5,
                bays: 3,
                floors_visible: 3,
                ox: 0,
            },
            notice: None,
            layout_key: String::new(),
            big_stars: None,
            floor_stars: Vec::new(),
        }
    }

    // ---------- layout ----------

    pub fn apply_layout(&mut self, w: &mut World, cols: u16, rows: u16) {
        let per_floor = ((cols as i32 - 2) / DESK_W).max(1) as usize;
        let bays = ((cols as i32 - 1) / BAY_W).clamp(1, 3) as usize;
        let scene_w = bays as i32 * BAY_W + 1;
        let floors_visible = ((rows as i32 - 2) / FLOOR_H).max(1) as usize;

        if self.cursor.as_ref().is_none_or(|c| w.knight(c).is_none()) {
            self.cursor = w.knights.first().map(|k| k.pane_id.clone());
        }

        let mut l = Layout {
            mode: Mode::Overview,
            shown: Vec::new(),
            floor_ws: None,
            start: 0,
            total: 0,
            cols,
            rows,
            per_floor,
            bays,
            floors_visible,
            ox: ((cols as i32 - scene_w) / 2).max(0),
        };
        if let Some(f) = self.focus.as_ref().and_then(|id| w.knight(id)) {
            let fl: Vec<&Knight> = w.knights.iter().filter(|k| k.workspace_id == f.workspace_id).collect();
            let i = fl.iter().position(|k| k.pane_id == f.pane_id).unwrap_or(0);
            let start = (i as i32 - 1).min(fl.len() as i32 - bays as i32).max(0) as usize;
            l.mode = Mode::Large { focus: true };
            l.shown = fl.iter().skip(start).take(bays).map(|k| k.pane_id.clone()).collect();
            l.floor_ws = Some(f.workspace_id.clone());
            l.start = start;
            l.total = fl.len();
        } else {
            self.focus = None;
            if w.knights.len() <= bays {
                l.mode = Mode::Large { focus: false };
                l.shown = w.knights.iter().map(|k| k.pane_id.clone()).collect();
            }
        }

        // Keep the cursor's floor on screen in the overview.
        let floors = w.floors(per_floor);
        if let Some(c) = &self.cursor {
            if let Some(fi) = floors.iter().position(|f| f.knights.contains(c)) {
                if fi < self.scroll {
                    self.scroll = fi;
                } else if fi >= self.scroll + floors_visible {
                    self.scroll = fi + 1 - floors_visible;
                }
            }
        }
        self.scroll = self.scroll.min(floors.len().saturating_sub(floors_visible));

        w.scene_w = scene_w;
        let key = format!("{:?}:{}:{}", l.mode, l.shown.join(","), bays);
        if key != self.layout_key {
            self.layout_key = key;
            let before: Vec<String> = w
                .knights
                .iter()
                .filter(|k| k.bay.is_some())
                .map(|k| k.pane_id.clone())
                .collect();
            for k in w.knights.iter_mut() {
                k.bay = l.shown.iter().position(|id| *id == k.pane_id);
            }
            for (bay, id) in l.shown.iter().enumerate() {
                anim::snap(w, id, bay);
            }
            for id in before.iter().filter(|id| !l.shown.contains(id)) {
                anim::settle(w, id);
            }
        }
        self.layout = l;
    }

    pub fn tick(&mut self, w: &mut World, dt: f32) {
        if let Some((_, t)) = &mut self.notice {
            *t -= dt;
            if *t <= 0.0 {
                self.notice = None;
            }
        }
        let l = &self.layout;
        let sw = w.scene_w;
        if self.big_stars.is_none() {
            self.big_stars = Some(Starfield::new(w, 46, 12));
        }
        let shown_working = l
            .shown
            .iter()
            .filter(|id| w.knight(id).is_some_and(|k| k.state == KnightState::Working))
            .count();
        if let Some(sf) = &mut self.big_stars {
            sf.advance(&mut w.rng, dt, 0.25 + shown_working as f32 * 0.65, (sw - 10) as f32, 12);
        }
        let floors = w.floors(l.per_floor);
        while self.floor_stars.len() < l.floors_visible {
            let sf = Starfield::new(w, 16, 4);
            self.floor_stars.push(sf);
        }
        let width = l.cols as f32;
        for (slot, f) in floors.iter().skip(self.scroll).take(l.floors_visible).enumerate() {
            let working = f
                .knights
                .iter()
                .filter(|id| w.knight(id).is_some_and(|k| k.state == KnightState::Working))
                .count();
            self.floor_stars[slot].advance(&mut w.rng, dt, 0.2 + working as f32 * 0.45, width, 4);
        }
    }

    // ---------- input ----------

    pub fn move_cursor(&mut self, w: &World, dx: i32, dy: i32) {
        self.kb_used = true;
        let Some(cur) = self.cursor.clone() else { return };
        match self.layout.mode {
            Mode::Overview => {
                let floors = w.floors(self.layout.per_floor);
                let Some(fi) = floors.iter().position(|f| f.knights.contains(&cur)) else {
                    return;
                };
                let pos = floors[fi].knights.iter().position(|k| *k == cur).unwrap_or(0) as i32;
                if dy != 0 {
                    let nf = (fi as i32 + dy).clamp(0, floors.len() as i32 - 1) as usize;
                    let row = &floors[nf].knights;
                    self.cursor = Some(row[(pos as usize).min(row.len() - 1)].clone());
                } else {
                    // Left and right walk through every knight in floor order.
                    let all: Vec<&String> = floors.iter().flat_map(|f| f.knights.iter()).collect();
                    let i = all.iter().position(|k| **k == cur).unwrap_or(0) as i32;
                    self.cursor = Some(all[(i + dx).clamp(0, all.len() as i32 - 1) as usize].clone());
                }
            }
            Mode::Large { focus } => {
                if dx == 0 {
                    return;
                }
                let pool: Vec<&Knight> = match focus {
                    true => {
                        let ws = self.layout.floor_ws.clone().unwrap_or_default();
                        w.knights.iter().filter(|k| k.workspace_id == ws).collect()
                    }
                    false => w.knights.iter().collect(),
                };
                let i = pool.iter().position(|k| k.pane_id == cur).unwrap_or(0) as i32;
                let next = pool[(i + dx).clamp(0, pool.len() as i32 - 1) as usize].pane_id.clone();
                if focus {
                    self.focus = Some(next.clone());
                }
                self.cursor = Some(next);
            }
        }
    }

    /// Which knight is under a cell in the overview.
    pub fn knight_at(&self, w: &World, col: u16, row: u16) -> Option<String> {
        if self.layout.mode != Mode::Overview {
            return None;
        }
        let floors = w.floors(self.layout.per_floor);
        let (col, row) = (col as i32, row as i32);
        for (slot, f) in floors
            .iter()
            .skip(self.scroll)
            .take(self.layout.floors_visible)
            .enumerate()
        {
            let r0 = 2 + slot as i32 * FLOOR_H;
            if row >= r0 && row <= r0 + 8 {
                let pos = ((col - 1) / DESK_W).max(0) as usize;
                return f.knights.get(pos).cloned();
            }
        }
        None
    }

    // ---------- drawing ----------

    pub fn draw(&self, w: &World, cv: &mut Canvas) {
        let l = &self.layout;
        match l.mode {
            Mode::Overview => {
                cv.begin(l.cols, l.rows, Rgb::hex(0x0b0f1e));
                self.draw_overview(w, cv);
            }
            Mode::Large { focus } => {
                cv.begin(l.cols, l.rows, WALL);
                self.draw_large(w, cv, focus);
            }
        }
        if let Some((msg, _)) = &self.notice {
            let row = l.rows as i32 - 2;
            cv.text(1, row, &format!(" {msg} "), AMBER_INK, AMBER);
        }
    }

    fn header(&self, w: &World, cv: &mut Canvas, left: &str) {
        let clock = format!(" ship time {} ", local_clock());
        let room = (cv.cols - clock.len() as i32).max(0) as usize;
        cv.text(0, 0, &fit("", cv.cols as usize), HEAD_FG, HB);
        cv.text(0, 0, &fit(left, room), HEAD_FG, HB);
        if !w.online {
            let msg = format!(" herdr: {} ", w.status);
            cv.text(0, 0, &fit(&msg, room), AMBER_INK, AMBER);
        }
        cv.text(room as i32, 0, &clock, MUTED, HB);
    }

    fn roster(&self, w: &World, cv: &mut Canvas) {
        let row = cv.rows - 1;
        cv.text(0, row, &fit("", cv.cols as usize), HEAD_FG, HB);
        let focus_ws = match self.layout.mode {
            Mode::Large { focus: true } => self.layout.floor_ws.clone(),
            _ => None,
        };
        let mut col = 1;
        let mut i = 0;
        while i < w.knights.len() {
            let ws = &w.knights[i].workspace_id;
            let here = focus_ws.as_deref() == Some(ws.as_str());
            col = cv.text(
                col,
                row,
                &format!("{ws} "),
                if here { Rgb::hex(0xdfe7ff) } else { MUTED },
                HB,
            );
            while i < w.knights.len() && &w.knights[i].workspace_id == ws {
                let k = &w.knights[i];
                let bl = k.state == KnightState::Blocked;
                col = cv.text(
                    col,
                    row,
                    k.state.glyph(),
                    if bl { AMBER_INK } else { k.crystal },
                    if bl { AMBER } else { HB },
                );
                i += 1;
            }
            col = cv.text(col, row, "   ", HB, HB);
        }
        let n_app = w.apps.iter().filter(|p| p.phase != Phase::Exit).count();
        let right = format!("knights {}  apprentices {} ", w.knights.len(), n_app);
        cv.text(cv.cols - right.len() as i32, row, &right, MUTED, HB);
    }

    fn bubble(&self, cv: &mut Canvas, x: i32, y: i32, alert: bool, blink: bool) {
        cv.rect(x + 1, y, 5, 1, BUBBLE);
        cv.rect(x, y + 1, 7, 4, BUBBLE);
        cv.rect(x + 1, y + 5, 5, 1, BUBBLE);
        cv.pi(x + 6, y + 6, BUBBLE);
        cv.pi(x + 7, y + 6, BUBBLE);
        if alert {
            if blink {
                cv.rect(x + 3, y + 1, 1, 2, RED);
                cv.pi(x + 3, y + 4, RED);
            }
        } else {
            for (dx, dy) in [(1, 3), (2, 4), (3, 3), (4, 2), (5, 1)] {
                cv.pi(x + dx, y + dy, GREEN);
            }
        }
    }

    fn knight_pal(&self, w: &World, k: &Knight) -> crate::render::Pal {
        let flash_on = k.flash > 0.0 && (w.time * 8.0) as i64 % 2 == 0;
        let mut pal = sprites::knight_pal(k.crystal, k.skin, flash_on);
        if k.state == KnightState::Unknown {
            // Hood pulled down over the face.
            pal = crate::render::Pal::of(&[
                ('h', Rgb::hex(0x1c1f40)),
                ('H', Rgb::hex(0x363a6a)),
                ('T', Rgb::hex(0x9a9cab)),
                ('B', Rgb::hex(0x5b4a3a)),
                ('C', k.crystal.mix(BLACK, 0.5)),
            ]);
        }
        pal
    }

    // ---------- large view ----------

    fn draw_bg(&self, w: &World, cv: &mut Canvas) {
        let sw = w.scene_w;
        cv.rect(0, 0, sw, 38, WALL);
        let mut x = 0;
        while x < sw {
            cv.rect(x, 2, 1, 34, Rgb::hex(0x222a45));
            x += 16;
        }
        cv.rect(4, 3, sw - 8, 14, FRAME);
        cv.rect(5, 4, sw - 10, 12, SPACE);
        let shown_working = self
            .layout
            .shown
            .iter()
            .filter(|id| w.knight(id).is_some_and(|k| k.state == KnightState::Working))
            .count();
        let warp = 0.25 + shown_working as f32 * 0.65;
        if let Some(sf) = &self.big_stars {
            let span = (sw - 11) as f32;
            for s in &sf.stars {
                let sx = 5.0 + s.x * span;
                let len = 1 + (warp * s.s * 1.6).round() as i32;
                cv.p(sx, (4 + s.y) as f32, Rgb::hex(0xe6f1ff));
                for k in 1..=len {
                    let xx = sx.round() as i32 + k;
                    if xx <= sw - 6 {
                        cv.pi(xx, 4 + s.y, Rgb::hex(0x9fc2ff).mix(SPACE, k as f32 / (len + 1) as f32));
                    }
                }
            }
        }
        for b in 1..self.layout.bays as i32 {
            cv.rect(b * BAY_W + 2, 4, 1, 12, FRAME);
        }
        cv.rect(0, 36, sw, 2, Rgb::hex(0x2a3354));
        let mut x = 4;
        while x < sw {
            let on = ((w.time * 1.5) as i32 + x / 12) % 4 == 0;
            cv.pi(x, 36, if on { Rgb::hex(0x8ab8ff) } else { Rgb::hex(0x3a4a78) });
            x += 12;
        }
        // Floor tiles run to the bottom of the pane, wider than the scene.
        for y in 38..cv.h {
            for x in -cv.ox..cv.w - cv.ox {
                let odd = (((x + cv.ox) >> 3) + (y >> 2)) & 1 == 1;
                cv.pi(x, y, if odd { Rgb::hex(0x151b2d) } else { Rgb::hex(0x192036) });
            }
        }
        cv.rect(-cv.ox, 38, cv.w, 1, Rgb::hex(0x0e1322));
    }

    fn draw_desk(&self, w: &World, cv: &mut Canvas, x: i32, y: i32, color: Rgb, on: bool) {
        let pick = |a: u32, b: u32| if on { Rgb::hex(a) } else { Rgb::hex(b) };
        cv.rect(x, y, 19, 1, pick(0x9aa6c4, 0x4a5470));
        cv.rect(x, y + 1, 19, 1, pick(0x66739a, 0x353d56));
        cv.rect(x, y + 2, 19, 6, pick(0x2f3a5c, 0x232a40));
        cv.rect(x, y + 2, 1, 6, Rgb::hex(0x46537a));
        cv.rect(x + 18, y + 2, 1, 6, Rgb::hex(0x46537a));
        cv.rect(x, y + 8, 19, 1, Rgb::hex(0x121728));
        if on {
            for i in 0..4 {
                let lit = ((w.time * 3.0 + i as f32 * 1.7 + x as f32) as i32) % 3 != 0;
                cv.pi(x + 3 + i * 2, y + 5, if lit { color } else { color.mix(BLACK, 0.65) });
            }
            cv.rect(x + 12, y + 4, 4, 1, Rgb::hex(0x3c4870));
            cv.rect(x + 12, y + 6, 4, 1, Rgb::hex(0x3c4870));
        }
    }

    fn draw_seat_terminal(&self, w: &World, cv: &mut Canvas, x: i32, color: Rgb, on: bool, busy: bool) {
        cv.rect(
            x + 1,
            46,
            7,
            1,
            if on { Rgb::hex(0x8a97b8) } else { Rgb::hex(0x4a5470) },
        );
        cv.rect(
            x + 1,
            47,
            7,
            3,
            if on { Rgb::hex(0x2f3a5c) } else { Rgb::hex(0x232a40) },
        );
        cv.rect(x + 1, 50, 7, 1, Rgb::hex(0x121728));
        let lamp = if busy {
            if (w.time * 4.0) as i32 % 2 == 1 {
                color
            } else {
                color.mix(BLACK, 0.5)
            }
        } else {
            Rgb::hex(0x3a4466)
        };
        cv.pi(x + 4, 48, lamp);
    }

    fn draw_app(&self, w: &World, cv: &mut Canvas, p: &Apprentice) {
        let Some(k) = w.knight(&p.master) else { return };
        let pal = apprentice_pal(k.crystal, p.skin, p.hair);
        let (x, y) = (p.x.round() as i32, p.y.round() as i32);
        if p.seated {
            cv.sprite(&APPRENTICE_SEATED, &pal, x, y + 4, SpriteOpts::default());
            let glow = ((w.time * 3.0) as i64 + p.id as i64) % 3 != 0;
            cv.rect(
                x + 3,
                y + 11,
                3,
                1,
                if glow { k.crystal } else { k.crystal.mix(BLACK, 0.5) },
            );
            cv.pi(x + 2, y + 11, p.skin);
            cv.pi(x + 6, y + 11, p.skin);
            return;
        }
        let mut rows: Vec<&str> = APPRENTICE.to_vec();
        if !p.path.is_empty() && (p.walk_t * 6.0) as i32 % 2 == 1 {
            rows[10] = APPRENTICE_STRIDE[0];
            rows[11] = APPRENTICE_STRIDE[1];
        }
        let bow = p.phase == Phase::Bow && p.bow_t > 0.25 && p.bow_t < 1.3;
        cv.sprite(
            &rows,
            &pal,
            x,
            y,
            SpriteOpts {
                back: p.back,
                bow,
                closed: false,
            },
        );
        if p.phase == Phase::Report {
            cv.rect(x + 3, y - 3, 2, 2, k.crystal);
            cv.pi(x + 3, y - 3, WHITE);
        }
        if p.phase == Phase::Bow && p.bow_t > 1.0 {
            // The cube floats from the apprentice's hands to the knight.
            let t = ((1.6 - p.bow_t) / 0.6).min(1.0);
            let st = station(k.bay.unwrap_or(0));
            let cx = (x + 3) as f32 + ((st.kx + 5 - (x + 3)) as f32) * t;
            let cy = (y - 3) as f32 + ((40 - (y - 3)) as f32) * t;
            cv.rect(cx.round() as i32, cy.round() as i32, 2, 2, k.crystal);
            cv.p(cx, cy, WHITE);
        }
    }

    fn draw_station(&self, w: &World, cv: &mut Canvas, k: Option<&Knight>, bay: usize) {
        let st = station(bay);
        let (sx, desk_x, kx) = (st.sx, st.desk_x, st.kx);
        let off = Rgb::hex(0x3a4466);
        let Some(k) = k else {
            self.draw_desk(w, cv, desk_x, 42, off, false);
            self.draw_seat_terminal(w, cv, sx, off, false, false);
            self.draw_seat_terminal(w, cv, sx + 28, off, false, false);
            cv.text(desk_x, 11, &fit("   - vacant bay -", 19), Rgb::hex(0x3c4668), WALL);
            return;
        };
        let t = w.time;
        let beam = k.crystal.mix(WALL, 0.7);
        for y in (28..42).step_by(2) {
            cv.pi(desk_x + 1, y, beam);
            cv.pi(desk_x + 17, y, beam);
        }
        let mut ky = 31;
        let mut closed = false;
        if k.state == KnightState::Idle {
            ky = 31 - ((t * 2.0 + bay as f32).sin() + 1.0).round() as i32;
            closed = true;
        }
        let pal = self.knight_pal(w, k);
        let hooded = k.state == KnightState::Unknown;
        cv.sprite(
            &KNIGHT,
            &pal,
            kx,
            ky,
            SpriteOpts {
                closed,
                back: hooded,
                bow: false,
            },
        );
        self.draw_desk(w, cv, desk_x, 42, k.crystal, true);
        let f = ((t * 8.0) as i32 + bay as i32) % 2;
        match k.state {
            KnightState::Working => {
                cv.rect(kx + 1, 42 - f, 2, 1, k.skin);
                cv.rect(kx + 9, 42 - (1 - f), 2, 1, k.skin);
            }
            KnightState::Blocked => {
                cv.rect(kx + 1, 42, 2, 1, k.skin);
                let wave = (t * 4.0) as i32 % 2;
                cv.rect(kx + 11, ky + 3, 2, 7, Rgb::hex(0x4a4f8c));
                cv.rect(kx + 11 + wave, ky + 1, 2, 2, k.skin);
                self.bubble(cv, kx - 9, 31, true, (t * 3.0) as i32 % 2 == 0);
            }
            KnightState::Done => {
                cv.rect(kx + 1, 42, 2, 1, k.skin);
                cv.rect(kx + 9, 42, 2, 1, k.skin);
                self.bubble(cv, kx - 9, 31, false, true);
            }
            KnightState::Idle => {
                cv.rect(kx + 5, ky + 10, 2, 1, k.skin);
                let (cx, cy) = ((kx + 6) as f32, (ky + 7) as f32);
                for i in 0..3 {
                    let ang = t * 1.6 + i as f32 * 2.1;
                    cv.p(cx + ang.cos() * 9.0, cy + ang.sin() * 4.0, k.crystal.mix(WHITE, 0.4));
                }
            }
            KnightState::Unknown => {}
        }

        let at_seat: Vec<&Apprentice> = w
            .crew(&k.pane_id)
            .filter(|p| p.phase == Phase::Work && matches!(p.spot, Spot::Seat(_)))
            .collect();
        for p in &at_seat {
            self.draw_app(w, cv, p);
        }
        self.draw_seat_terminal(
            w,
            cv,
            sx,
            k.crystal,
            true,
            at_seat.iter().any(|p| p.spot == Spot::Seat(0)),
        );
        self.draw_seat_terminal(
            w,
            cv,
            sx + 28,
            k.crystal,
            true,
            at_seat.iter().any(|p| p.spot == Spot::Seat(1)),
        );
        for p in &at_seat {
            let g = ((t * 6.0) as i64 + p.id as i64) % 2;
            let x = p.x.round() as i32;
            cv.pi(x + 1, 46 - g as i32, p.skin);
            cv.pi(x + 7, 46 - (1 - g as i32), p.skin);
        }

        // Holo screen above the desk.
        let blink = (t * 2.0) as i32 % 2 == 0;
        let (mut fg, mut bg) = (k.crystal, k.crystal.mix(SPACE, 0.86));
        let mut l1: String;
        let mut l2 = k.title.clone();
        match k.state {
            KnightState::Working => l1 = "> WORKING".into(),
            KnightState::Blocked => {
                fg = AMBER;
                bg = if blink { Rgb::hex(0x4a3510) } else { Rgb::hex(0x2e220c) };
                l1 = "!! NEEDS YOU".into();
            }
            KnightState::Done => {
                fg = Rgb::hex(0x9dffb0);
                bg = Rgb::hex(0x10301c);
                l1 = "* DONE".into();
                l2 = "unread".into();
            }
            KnightState::Idle => {
                fg = Rgb::hex(0x7f8bb0);
                bg = Rgb::hex(0x10162a);
                l1 = "~ MEDITATING".into();
                l2.clear();
            }
            KnightState::Unknown => {
                fg = Rgb::hex(0x8f97b0);
                bg = Rgb::hex(0x141826);
                l1 = "? UNKNOWN".into();
                let noise = ['░', '▒', '▓', ' ', '·'];
                let seed = (t * 10.0) as u64;
                l2 = (0..17)
                    .map(|i| noise[((seed * 31 + i * 17 + seed * i) % 5) as usize])
                    .collect();
            }
        }
        if k.flash > 0.0 && blink {
            l1 = "<< REPORT IN".into();
        }
        let spin = ['|', '/', '-', '\\'][(t * 6.0) as usize % 4];
        let mut crew: Vec<&Apprentice> = w.crew(&k.pane_id).filter(|p| p.phase != Phase::Exit).collect();
        crew.sort_by_key(|p| p.id);
        let hidden = crew
            .iter()
            .filter(|p| p.phase == Phase::Work && p.spot == Spot::Offdeck)
            .count();
        let sym = |p: &Apprentice| match p.phase {
            Phase::Enter | Phase::Move => '+',
            Phase::Work => spin,
            _ => '^',
        };
        let (mut l3, mut l4) = (String::new(), String::new());
        if crew.len() <= 2 {
            if let Some(p) = crew.first() {
                l3 = format!("{} {}", sym(p), p.task);
            }
            if let Some(p) = crew.get(1) {
                l4 = format!("{} {}", sym(p), p.task);
            }
        } else {
            l3 = format!("{} {}", sym(crew[0]), crew[0].task);
            l4 = format!(
                "+{} more{}",
                crew.len() - 1,
                if hidden > 0 {
                    format!(", {hidden} off")
                } else {
                    String::new()
                }
            );
        }
        let selected = self.kb_used && self.cursor.as_deref() == Some(k.pane_id.as_str());
        for (i, s) in [&k.name, &l1, &l2, &l3, &l4].iter().enumerate() {
            let (f, b) = match i {
                0 if selected => (bg, fg),
                0 => (fg.mix(WHITE, 0.35), bg),
                _ => (fg, bg),
            };
            cv.text(desk_x, 9 + i as i32, &fit(&format!(" {s}"), 19), f, b);
        }
        if hidden > 0 {
            cv.text(
                sx + 9,
                32,
                &fit(&format!(" +{hidden} offdeck"), 12),
                k.crystal,
                Rgb::hex(0x121828),
            );
        }
    }

    fn draw_large(&self, w: &World, cv: &mut Canvas, focus: bool) {
        let l = &self.layout;
        cv.ox = l.ox;
        self.draw_bg(w, cv);
        for bay in 0..l.bays {
            let k = l.shown.get(bay).and_then(|id| w.knight(id));
            self.draw_station(w, cv, k, bay);
        }
        // Walkers and floor-sitters, back to front by where their feet are.
        let mut walkers: Vec<&Apprentice> = w
            .apps
            .iter()
            .filter(|p| {
                w.knight(&p.master).is_some_and(|k| k.bay.is_some())
                    && !(p.phase == Phase::Work && matches!(p.spot, Spot::Seat(_) | Spot::Offdeck))
            })
            .collect();
        let feet = |p: &Apprentice| if p.seated { p.y + 12.0 } else { p.y + 11.0 };
        walkers.sort_by(|a, b| feet(a).total_cmp(&feet(b)));
        for p in walkers {
            self.draw_app(w, cv, p);
        }
        cv.ox = 0;
        if focus {
            let ws = l.floor_ws.as_deref().unwrap_or("");
            let end = l.start + l.shown.len();
            self.header(
                w,
                cv,
                &format!(
                    " STARKEEP  {} {}  bays {}-{} of {}   esc: all floors  enter: go to pane",
                    ws,
                    w.workspace_label(ws),
                    l.start + 1,
                    end,
                    l.total
                ),
            );
        } else if w.knights.is_empty() {
            self.header(w, cv, " STARKEEP  training floor   waiting for knights...");
        } else {
            self.header(w, cv, " STARKEEP  training floor   enter: go to pane  esc: close");
        }
        self.roster(w, cv);
    }

    // ---------- overview ----------

    fn draw_mini(&self, w: &World, cv: &mut Canvas, k: Option<&Knight>, pos: i32, py0: i32, r0: i32) {
        let sx = 1 + pos * DESK_W;
        let Some(k) = k else {
            cv.rect(sx + 1, py0 + 9, 9, 1, Rgb::hex(0x4a5470));
            cv.rect(sx + 1, py0 + 10, 9, 2, Rgb::hex(0x232a40));
            cv.text(
                sx,
                r0 + 8,
                &fit("   - vacant -", DESK_W as usize),
                Rgb::hex(0x3c4668),
                Rgb::hex(0x0e1324),
            );
            return;
        };
        let t = w.time;
        let mut ky = py0 + 3;
        let mut closed = false;
        if k.state == KnightState::Idle {
            ky -= ((t * 1.5) as i32 + pos) % 2;
            closed = true;
        }
        let pal = self.knight_pal(w, k);
        cv.sprite(
            &KNIGHT_MINI,
            &pal,
            sx + 2,
            ky,
            SpriteOpts {
                closed,
                back: k.state == KnightState::Unknown,
                bow: false,
            },
        );
        cv.rect(sx + 1, py0 + 9, 9, 1, Rgb::hex(0x9aa6c4));
        cv.rect(sx + 1, py0 + 10, 9, 2, Rgb::hex(0x2f3a5c));
        for i in 0..3 {
            let lit = ((t * 3.0 + i as f32 * 1.3 + sx as f32) as i32) % 3 != 0;
            cv.pi(
                sx + 3 + i * 2,
                py0 + 11,
                if lit { k.crystal } else { k.crystal.mix(BLACK, 0.6) },
            );
        }
        let g = ((t * 8.0) as i32 + k.color_idx as i32) % 2;
        match k.state {
            KnightState::Working => {
                cv.pi(sx + 3, py0 + 9 - g, k.skin);
                cv.pi(sx + 7, py0 + 9 - (1 - g), k.skin);
            }
            KnightState::Blocked => {
                cv.pi(sx + 9, ky + 1 - ((t * 4.0) as i32 % 2), k.skin);
                cv.rect(sx + 9, ky + 2, 1, 3, Rgb::hex(0x4a4f8c));
                cv.rect(sx + 10, py0, 3, 5, BUBBLE);
                if (t * 3.0) as i32 % 2 == 0 {
                    cv.pi(sx + 11, py0 + 1, RED);
                    cv.pi(sx + 11, py0 + 2, RED);
                    cv.pi(sx + 11, py0 + 4, RED);
                }
            }
            KnightState::Done => {
                cv.rect(sx + 10, py0, 3, 5, BUBBLE);
                cv.pi(sx + 10, py0 + 2, GREEN);
                cv.pi(sx + 11, py0 + 3, GREEN);
                cv.pi(sx + 12, py0 + 1, GREEN);
            }
            KnightState::Idle => {
                let ang = t * 1.6 + pos as f32;
                cv.p(
                    (sx + 5) as f32 + ang.cos() * 5.0,
                    (ky + 4) as f32 + ang.sin() * 3.0,
                    k.crystal.mix(WHITE, 0.4),
                );
            }
            KnightState::Unknown => {}
        }
        // Apprentices in miniature: reporters first, then by arrival.
        let crew: Vec<&Apprentice> = w.crew(&k.pane_id).filter(|p| p.phase != Phase::Exit).collect();
        let mut order: Vec<&Apprentice> = crew.iter().copied().filter(|p| !p.active()).collect();
        let mut working: Vec<&Apprentice> = crew.iter().copied().filter(|p| p.active()).collect();
        working.sort_by_key(|p| p.id);
        order.extend(working);
        for (n, p) in order.iter().take(3).enumerate() {
            let x = sx + 10 + n as i32 * 4;
            let y = py0 + 7;
            let back = !p.active();
            let bob = if !back && ((t * 4.0) as i64 + p.id as i64) % 2 == 1 {
                1
            } else {
                0
            };
            cv.sprite(
                &APPRENTICE_MINI,
                &apprentice_pal(k.crystal, p.skin, p.hair),
                x,
                y - bob,
                SpriteOpts {
                    back,
                    ..Default::default()
                },
            );
            if back && (t * 4.0) as i32 % 2 == 1 {
                cv.rect(x + 1, y - 2, 2, 1, k.crystal);
            }
        }
        let extra = order.len().saturating_sub(3);
        let right = if extra > 0 { format!("+{extra} ") } else { String::new() };
        let label = fit(
            &format!(" {} {}", k.state.glyph(), k.name),
            DESK_W as usize - right.len(),
        ) + &right;
        let (mut fg, mut bg) = (k.crystal, Rgb::hex(0x0e1324));
        match k.state {
            KnightState::Blocked => {
                fg = AMBER_INK;
                bg = if (t * 2.0) as i32 % 2 == 0 { AMBER } else { AMBER_DIM };
            }
            KnightState::Idle | KnightState::Unknown => fg = k.crystal.mix(Rgb::hex(0x0e1324), 0.35),
            _ => {}
        }
        if self.kb_used && self.cursor.as_deref() == Some(k.pane_id.as_str()) && k.state != KnightState::Blocked {
            bg = Rgb::hex(0x2a3766);
        }
        cv.text(sx, r0 + 8, &label, fg, bg);
    }

    fn draw_overview(&self, w: &World, cv: &mut Canvas) {
        let l = &self.layout;
        let floors: Vec<Floor> = w.floors(l.per_floor);
        let cols = cv.cols;
        for (slot, f) in floors.iter().skip(self.scroll).take(l.floors_visible).enumerate() {
            let r0 = 2 + slot as i32 * FLOOR_H;
            let py0 = (r0 + 1) * 2;
            cv.rect(0, py0, cv.w, 9, WALL);
            cv.rect(1, py0 + 1, cv.w - 2, 4, SPACE);
            if let Some(sf) = self.floor_stars.get(slot) {
                for s in &sf.stars {
                    let sx = 2.0 + s.x * (cv.w - 5) as f32;
                    let len = 1 + s.s.round() as i32;
                    let y = py0 + 1 + s.y;
                    cv.p(sx, y as f32, Rgb::hex(0xe6f1ff));
                    for k in 1..=len {
                        let xx = sx.round() as i32 + k;
                        if xx <= cv.w - 2 {
                            cv.pi(xx, y, Rgb::hex(0x9fc2ff).mix(SPACE, k as f32 / (len + 1) as f32));
                        }
                    }
                }
            }
            for i in 0..=l.per_floor as i32 {
                cv.rect(i * DESK_W, py0 + 1, 1, 4, FRAME);
            }
            cv.rect(0, py0 + 8, cv.w, 1, Rgb::hex(0x2a3354));
            for y in py0 + 9..py0 + 14 {
                for x in 0..cv.w {
                    let odd = ((x >> 3) + (y >> 1)) & 1 == 1;
                    cv.pi(x, y, if odd { Rgb::hex(0x151b2d) } else { Rgb::hex(0x192036) });
                }
            }
            for pos in 0..l.per_floor {
                let k = f.knights.get(pos).and_then(|id| w.knight(id));
                self.draw_mini(w, cv, k, pos as i32, py0, r0);
            }

            let ks: Vec<&Knight> = f.knights.iter().filter_map(|id| w.knight(id)).collect();
            let working = ks.iter().filter(|k| k.state == KnightState::Working).count();
            let blocked = ks.iter().filter(|k| k.state == KnightState::Blocked).count();
            let apps = f
                .knights
                .iter()
                .map(|id| w.crew(id).filter(|p| p.phase != Phase::Exit).count())
                .sum::<usize>();
            let left = format!(" {} {}", f.workspace_id, f.name);
            cv.text(0, r0, &fit(&left, cols as usize), HEAD_FG, HB);
            if blocked > 0 {
                cv.text(crate::render::str_width(&left) as i32 + 1, r0, " ! ", AMBER_INK, AMBER);
            }
            let right = format!(
                "{} knights  {} working  {}{} apprentices ",
                ks.len(),
                working,
                if blocked > 0 {
                    format!("{blocked} waiting  ")
                } else {
                    String::new()
                },
                apps
            );
            cv.text(cols - right.len() as i32, r0, &right, MUTED, HB);
        }

        let blocked_in = |fs: &[Floor]| {
            fs.iter()
                .flat_map(|f| f.knights.iter())
                .filter(|id| w.knight(id).is_some_and(|k| k.state == KnightState::Blocked))
                .count()
        };
        let above = blocked_in(&floors[..self.scroll.min(floors.len())]);
        let below = blocked_in(floors.get(self.scroll + l.floors_visible..).unwrap_or(&[]));
        let mut title = " STARKEEP  all floors   click or enter: zoom in  esc: close".to_string();
        if floors.len() > l.floors_visible {
            title += &format!(
                "   floors {}-{} of {}",
                self.scroll + 1,
                (self.scroll + l.floors_visible).min(floors.len()),
                floors.len()
            );
        }
        self.header(w, cv, &title);
        if above > 0 || below > 0 {
            let mut s = String::new();
            if above > 0 {
                s += &format!(" ▲ {above} waiting ");
            }
            if below > 0 {
                s += &format!(" ▼ {below} waiting ");
            }
            let col = cols - 22 - crate::render::str_width(&s) as i32;
            cv.text(col, 0, &s, AMBER_INK, AMBER);
        }
        self.roster(w, cv);
    }
}

fn local_clock() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as libc::time_t)
        .unwrap_or(0);
    // SAFETY: localtime_r only writes into the provided struct.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut tm) };
    format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
}
