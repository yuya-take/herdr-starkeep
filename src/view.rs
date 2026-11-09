//! The two zoom levels: a grid of rooms, one per herdr Space, and a close-up
//! of up to three bays in one Space. Picks which one to show and draws it.

use crate::anim::{self, Starfield};
use crate::model::{Apprentice, Knight, KnightState, Phase, Room, Spot, World};
use crate::render::{fit, Canvas, Rgb, SpriteOpts};
use crate::spots::{station, BAY_W};
use crate::sprites::{self, *};

/// Desks drawn per room; more knights than this show as "+N".
pub const MAX_DESKS: usize = 4;
/// Rooms shown at once: up to 2 rows of 3.
const MAX_GRID_ROWS: usize = 2;
const MAX_GRID_COLS: usize = 3;

const HB: Rgb = Rgb::hex(0x0c1020);
const HEAD_FG: Rgb = Rgb::hex(0x9fb3e0);
const MUTED: Rgb = Rgb::hex(0x6f7fa8);
const SELECT: Rgb = Rgb::hex(0x2a3766);
const AMBER: Rgb = Rgb::hex(0xffd166);
const AMBER_DIM: Rgb = Rgb::hex(0xb8892c);
const AMBER_INK: Rgb = Rgb::hex(0x1a1200);
const BUBBLE: Rgb = Rgb::hex(0xeef1f8);
const RED: Rgb = Rgb::hex(0xe5484d);
const GREEN: Rgb = Rgb::hex(0x2fae5b);
const SPACE: Rgb = Rgb::hex(0x05080f);
const WALL: Rgb = Rgb::hex(0x1a2036);
const FRAME: Rgb = Rgb::hex(0x2e3858);
const GAP: Rgb = Rgb::hex(0x0b0f1e);
const TAG_BG: Rgb = Rgb::hex(0x0e1324);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Every Space as a room in a grid.
    Rooms,
    /// Close-up of the focused knight's Space.
    Zoom,
}

pub struct Layout {
    pub mode: Mode,
    /// pane ids shown in the close-up, one per bay
    pub shown: Vec<String>,
    pub zoom_ws: Option<String>,
    pub start: usize,
    pub total: usize,
    pub cols: u16,
    pub rows: u16,
    pub bays: usize,
    pub grid_rows: usize,
    pub grid_cols: usize,
    pub tile_w: i32,
    pub tile_h: i32,
    /// Left offset that centres the close-up scene.
    pub ox: i32,
}

pub struct View {
    /// Knight shown in the close-up.
    pub focus: Option<String>,
    /// Selected room (workspace id) in the grid.
    pub room: Option<String>,
    pub kb_used: bool,
    /// First visible grid row.
    pub scroll: usize,
    pub layout: Layout,
    pub notice: Option<(String, f32)>,
    layout_key: String,
    big_stars: Option<Starfield>,
    room_stars: Vec<Starfield>,
}

/// Grid shape for `n` rooms: 1x1, 1x2, 1x3, 2x2, then 2x3 (scrolling past 6).
/// Narrow or short panes get fewer columns or rows.
fn grid_shape(n: usize, cols: u16, rows: u16) -> (usize, usize) {
    let (mut gr, mut gc) = match n {
        0 | 1 => (1, 1),
        2 => (1, 2),
        3 => (1, 3),
        4 => (2, 2),
        _ => (MAX_GRID_ROWS, MAX_GRID_COLS),
    };
    let max_c = (cols as usize / 28).clamp(1, MAX_GRID_COLS);
    let max_r = ((rows as usize).saturating_sub(2) / 9).clamp(1, MAX_GRID_ROWS);
    if gc > max_c {
        gc = max_c;
        gr = n.div_ceil(gc).clamp(1, MAX_GRID_ROWS);
    }
    (gr.min(max_r), gc)
}

impl View {
    pub fn new() -> View {
        View {
            focus: None,
            room: None,
            kb_used: false,
            scroll: 0,
            layout: Layout {
                mode: Mode::Rooms,
                shown: Vec::new(),
                zoom_ws: None,
                start: 0,
                total: 0,
                cols: 112,
                rows: 34,
                bays: 3,
                grid_rows: 1,
                grid_cols: 1,
                tile_w: 112,
                tile_h: 32,
                ox: 0,
            },
            notice: None,
            layout_key: String::new(),
            big_stars: None,
            room_stars: Vec::new(),
        }
    }

    // ---------- layout ----------

    pub fn apply_layout(&mut self, w: &mut World, cols: u16, rows: u16) {
        let bays = ((cols as i32 - 1) / BAY_W).clamp(1, 3) as usize;
        let scene_w = bays as i32 * BAY_W + 1;
        let rooms = w.rooms();
        if self
            .room
            .as_ref()
            .is_none_or(|r| !rooms.iter().any(|x| &x.workspace_id == r))
        {
            self.room = rooms.first().map(|r| r.workspace_id.clone());
        }
        let (grid_rows, grid_cols) = grid_shape(rooms.len(), cols, rows);

        let mut l = Layout {
            mode: Mode::Rooms,
            shown: Vec::new(),
            zoom_ws: None,
            start: 0,
            total: 0,
            cols,
            rows,
            bays,
            grid_rows,
            grid_cols,
            tile_w: cols as i32 / grid_cols as i32,
            tile_h: (rows as i32 - 2) / grid_rows as i32,
            ox: ((cols as i32 - scene_w) / 2).max(0),
        };
        if let Some(f) = self.focus.as_ref().and_then(|id| w.knight(id)) {
            let fl: Vec<&Knight> = w.knights.iter().filter(|k| k.workspace_id == f.workspace_id).collect();
            let i = fl.iter().position(|k| k.pane_id == f.pane_id).unwrap_or(0);
            let start = (i as i32 - 1).min(fl.len() as i32 - bays as i32).max(0) as usize;
            l.mode = Mode::Zoom;
            l.shown = fl.iter().skip(start).take(bays).map(|k| k.pane_id.clone()).collect();
            l.zoom_ws = Some(f.workspace_id.clone());
            l.start = start;
            l.total = fl.len();
        } else {
            self.focus = None;
        }

        // Keep the selected room's grid row on screen.
        let total_rows = rooms.len().div_ceil(grid_cols);
        if let Some(ri) = self
            .room
            .as_ref()
            .and_then(|r| rooms.iter().position(|x| &x.workspace_id == r))
        {
            let row = ri / grid_cols;
            if row < self.scroll {
                self.scroll = row;
            } else if row >= self.scroll + grid_rows {
                self.scroll = row + 1 - grid_rows;
            }
        }
        self.scroll = self.scroll.min(total_rows.saturating_sub(grid_rows));

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

    /// Rooms currently on screen, with their tile index.
    fn visible_rooms(&self, rooms: &[Room]) -> std::ops::Range<usize> {
        let per = self.layout.grid_cols;
        let a = (self.scroll * per).min(rooms.len());
        let b = ((self.scroll + self.layout.grid_rows) * per).min(rooms.len());
        a..b
    }

    pub fn tick(&mut self, w: &mut World, dt: f32) {
        if let Some((_, t)) = &mut self.notice {
            *t -= dt;
            if *t <= 0.0 {
                self.notice = None;
            }
        }
        let sw = w.scene_w;
        if self.big_stars.is_none() {
            self.big_stars = Some(Starfield::new(w, 46));
        }
        let working = |w: &World, ids: &[String]| {
            ids.iter()
                .filter(|id| w.knight(id).is_some_and(|k| k.state == KnightState::Working))
                .count() as f32
        };
        let shown_working = working(w, &self.layout.shown);
        if let Some(sf) = &mut self.big_stars {
            sf.advance(&mut w.rng, dt, 0.25 + shown_working * 0.65, (sw - 10) as f32);
        }
        let rooms = w.rooms();
        let tiles = self.layout.grid_rows * self.layout.grid_cols;
        while self.room_stars.len() < tiles {
            let sf = Starfield::new(w, 18);
            self.room_stars.push(sf);
        }
        let width = self.layout.tile_w as f32;
        for (tile, ri) in self.visible_rooms(&rooms).enumerate() {
            let warp = 0.2 + working(w, &rooms[ri].knights) * 0.45;
            self.room_stars[tile].advance(&mut w.rng, dt, warp, width);
        }
    }

    // ---------- input ----------

    pub fn move_cursor(&mut self, w: &World, dx: i32, dy: i32) {
        self.kb_used = true;
        match self.layout.mode {
            Mode::Rooms => {
                let rooms = w.rooms();
                if rooms.is_empty() {
                    return;
                }
                let i = self
                    .room
                    .as_ref()
                    .and_then(|r| rooms.iter().position(|x| &x.workspace_id == r))
                    .unwrap_or(0) as i32;
                let step = dx + dy * self.layout.grid_cols as i32;
                let next = (i + step).clamp(0, rooms.len() as i32 - 1) as usize;
                self.room = Some(rooms[next].workspace_id.clone());
            }
            Mode::Zoom => {
                let Some(cur) = self.focus.clone() else { return };
                if dx == 0 {
                    return;
                }
                let ws = self.layout.zoom_ws.clone().unwrap_or_default();
                let pool: Vec<&Knight> = w.knights.iter().filter(|k| k.workspace_id == ws).collect();
                let i = pool.iter().position(|k| k.pane_id == cur).unwrap_or(0) as i32;
                let next = pool[(i + dx).clamp(0, pool.len() as i32 - 1) as usize];
                self.focus = Some(next.pane_id.clone());
            }
        }
    }

    /// Zoom into a room, starting with a knight that needs an answer.
    pub fn enter_room(&mut self, w: &World, ws: &str, knight: Option<String>) {
        self.room = Some(ws.to_string());
        let ks: Vec<&Knight> = w.knights.iter().filter(|k| k.workspace_id == ws).collect();
        let pick = knight.or_else(|| {
            ks.iter()
                .find(|k| k.state == KnightState::Blocked)
                .or(ks.first())
                .map(|k| k.pane_id.clone())
        });
        match pick {
            Some(id) => self.focus = Some(id),
            None => self.notice = Some(("no knights in this space".into(), 1.5)),
        }
    }

    pub fn enter_selected(&mut self, w: &World) {
        if let Some(ws) = self.room.clone() {
            self.kb_used = true;
            self.enter_room(w, &ws, None);
        }
    }

    /// Room and knight under a cell in the grid.
    pub fn hit(&self, w: &World, col: u16, row: u16) -> Option<(String, Option<String>)> {
        if self.layout.mode != Mode::Rooms {
            return None;
        }
        let l = &self.layout;
        let (col, row) = (col as i32, row as i32 - 1);
        if row < 0 {
            return None;
        }
        let (gc, gr) = (col / l.tile_w, row / l.tile_h);
        if gc >= l.grid_cols as i32 || gr >= l.grid_rows as i32 {
            return None;
        }
        let rooms = w.rooms();
        let ri = (self.scroll + gr as usize) * l.grid_cols + gc as usize;
        let room = rooms.get(ri)?;
        let d = room.knights.len().clamp(1, MAX_DESKS) as i32;
        let slot_w = ((l.tile_w - 3) / d).max(1);
        let slot = ((col - gc * l.tile_w - 1) / slot_w).clamp(0, d - 1) as usize;
        Some((room.workspace_id.clone(), room.knights.get(slot).cloned()))
    }

    // ---------- drawing ----------

    pub fn draw(&self, w: &World, cv: &mut Canvas) {
        let l = &self.layout;
        match l.mode {
            Mode::Rooms => {
                cv.begin(l.cols, l.rows, GAP);
                self.draw_rooms(w, cv);
            }
            Mode::Zoom => {
                cv.begin(l.cols, l.rows, WALL);
                self.draw_large(w, cv);
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
            Mode::Zoom => self.layout.zoom_ws.clone(),
            Mode::Rooms => None,
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
                let sy = 4 + (s.y * 12.0) as i32;
                cv.p(sx, sy as f32, Rgb::hex(0xe6f1ff));
                for k in 1..=len {
                    let xx = sx.round() as i32 + k;
                    if xx <= sw - 6 {
                        cv.pi(xx, sy, Rgb::hex(0x9fc2ff).mix(SPACE, k as f32 / (len + 1) as f32));
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
        let selected = self.focus.as_deref() == Some(k.pane_id.as_str());
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

    fn draw_large(&self, w: &World, cv: &mut Canvas) {
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
        let ws = l.zoom_ws.as_deref().unwrap_or("");
        let end = l.start + l.shown.len();
        self.header(
            w,
            cv,
            &format!(
                " STARKEEP  {} {}  bays {}-{} of {}   arrows: move  enter: go to pane  esc: back",
                ws,
                w.workspace_label(ws),
                l.start + 1,
                end,
                l.total
            ),
        );
        self.roster(w, cv);
    }

    // ---------- rooms ----------

    fn draw_rooms(&self, w: &World, cv: &mut Canvas) {
        let l = &self.layout;
        let rooms = w.rooms();
        let range = self.visible_rooms(&rooms);
        // Double-size sprites only when every room on screen has space for them,
        // so the rooms stay the same scale.
        let slot_w = |r: &Room| (l.tile_w - 3) / r.knights.len().clamp(1, MAX_DESKS) as i32;
        let big = (l.tile_h - 2) * 2 >= 44 && rooms[range.clone()].iter().all(|r| slot_w(r) >= 26);
        let scale = if big { 2 } else { 1 };
        for (tile, ri) in range.clone().enumerate() {
            let x0 = (tile % l.grid_cols) as i32 * l.tile_w;
            let y0 = 1 + (tile / l.grid_cols) as i32 * l.tile_h;
            let selected = self.kb_used && self.room.as_deref() == Some(rooms[ri].workspace_id.as_str());
            self.draw_room(w, cv, &rooms[ri], (tile, scale), (x0, y0), selected);
        }
        if rooms.is_empty() {
            let msg = "waiting for herdr spaces...";
            cv.text((cv.cols - msg.len() as i32) / 2, cv.rows / 2, msg, MUTED, GAP);
        }

        let total_rows = rooms.len().div_ceil(l.grid_cols);
        let mut title = format!(
            " STARKEEP  {} spaces   arrows: select  enter: zoom in  esc: close",
            rooms.len()
        );
        if total_rows > l.grid_rows {
            title += &format!(
                "   rows {}-{} of {}",
                self.scroll + 1,
                (self.scroll + l.grid_rows).min(total_rows),
                total_rows
            );
        }
        self.header(w, cv, &title);

        let blocked_in = |rs: &[Room]| {
            rs.iter()
                .flat_map(|r| r.knights.iter())
                .filter(|id| w.knight(id).is_some_and(|k| k.state == KnightState::Blocked))
                .count()
        };
        let above = blocked_in(&rooms[..range.start]);
        let below = blocked_in(&rooms[range.end..]);
        if above > 0 || below > 0 {
            let mut s = String::new();
            if above > 0 {
                s += &format!(" ▲ {above} waiting ");
            }
            if below > 0 {
                s += &format!(" ▼ {below} waiting ");
            }
            let col = cv.cols - 22 - crate::render::str_width(&s) as i32;
            cv.text(col, 0, &s, AMBER_INK, AMBER);
        }
        self.roster(w, cv);
    }

    /// One Space: header, window, a desk per knight (up to four), name tags.
    fn draw_room(&self, w: &World, cv: &mut Canvas, room: &Room, tile: (usize, i32), at: (i32, i32), selected: bool) {
        let ((tile, s), (x0, y0)) = (tile, at);
        let l = &self.layout;
        let rw = l.tile_w - 1; // the last column is the gap between rooms
        let tag_row = y0 + l.tile_h - 1;
        let pt = (y0 + 1) * 2; // first pixel row under the header
        let pb = tag_row * 2; // first pixel row of the name tags
        let n = room.knights.len();
        let d = n.min(MAX_DESKS);
        let slot_w = (rw - 2) / d.max(1) as i32;
        let b = pb - 14 * s; // top of the desk scene
        let ks: Vec<&Knight> = room.knights.iter().filter_map(|id| w.knight(id)).collect();

        // Each desk's share of the room is lit by its knight's state.
        let lights: Vec<Light> = match d {
            0 => vec![light(None, w.time)],
            _ => ks.iter().take(d).map(|k| light(Some(k.state), w.time)).collect(),
        };
        let slot_of = |x: i32| match d {
            0 => 0,
            _ => ((x - x0 - 1) / slot_w).clamp(0, d as i32 - 1) as usize,
        };

        // Wall, window and floor.
        let (wx, wy, ww) = (x0 + 1, pt + 1, rw - 2);
        let wh = (b + 5 * s - wy).max(3);
        for x in x0..x0 + rw {
            let lt = &lights[slot_of(x)];
            cv.rect(x, pt, 1, b + 8 * s - pt, lt.wall);
            cv.rect(x, b + 8 * s, 1, s, lt.rail);
            for y in b + 9 * s..pb {
                let odd = ((x >> 3) + ((y - pt) >> s)) & 1 == 1;
                cv.pi(x, y, if odd { lt.floor.0 } else { lt.floor.1 });
            }
        }
        cv.rect(wx, wy - 1, ww, 1, FRAME);
        cv.rect(wx, wy + wh, ww, 1, FRAME);
        for x in wx..wx + ww {
            cv.rect(
                x,
                wy,
                1,
                wh,
                SPACE.mix(Rgb::hex(0x03050a), 1.0 - lights[slot_of(x)].stars),
            );
        }
        // Ceiling lights over lit desks.
        for (i, lt) in lights.iter().enumerate() {
            if let Some(c) = lt.ceiling {
                let (sx, sw) = (x0 + 1 + i as i32 * slot_w, if d == 0 { rw - 2 } else { slot_w });
                cv.rect(sx + sw / 5, wy - 1, sw - 2 * (sw / 5), 1, c);
            }
        }
        if let Some(sf) = self.room_stars.get(tile) {
            for st in &sf.stars {
                let sx = wx as f32 + st.x * (ww - 1) as f32;
                let sy = wy + ((st.y * wh as f32) as i32).min(wh - 1);
                let bright = lights[slot_of(sx.round() as i32)].stars;
                let (head, tail) = (
                    SPACE.mix(Rgb::hex(0xe6f1ff), bright),
                    SPACE.mix(Rgb::hex(0x9fc2ff), bright),
                );
                let len = 1 + st.s.round() as i32;
                cv.p(sx, sy as f32, head);
                for k in 1..=len {
                    let xx = sx.round() as i32 + k;
                    if xx < wx + ww {
                        cv.pi(xx, sy, tail.mix(SPACE, k as f32 / (len + 1) as f32));
                    }
                }
            }
        }
        for i in 1..d as i32 {
            cv.rect(x0 + 1 + i * slot_w, wy, 1, wh, FRAME);
        }

        // Header: Space name, plus waiting and overflow badges.
        let (fg, bg) = if selected {
            (Rgb::hex(0xdfe7ff), SELECT)
        } else {
            (HEAD_FG, HB)
        };
        cv.text(
            x0,
            y0,
            &fit(&format!(" {} {}", room.workspace_id, room.name), rw as usize),
            fg,
            bg,
        );
        let mut badges: Vec<(String, Rgb, Rgb)> = Vec::new();
        let hidden = ks.len().saturating_sub(MAX_DESKS);
        if hidden > 0 {
            let hidden_waiting = ks[MAX_DESKS..].iter().any(|k| k.state == KnightState::Blocked);
            let (f, b) = if hidden_waiting {
                (AMBER_INK, AMBER)
            } else {
                (HEAD_FG, FRAME)
            };
            badges.push((format!(" +{hidden} more "), f, b));
        }
        if ks.iter().any(|k| k.state == KnightState::Blocked) {
            badges.push((" ! ".into(), AMBER_INK, AMBER));
        }
        let mut col = x0 + rw;
        for (text, f, b) in badges.iter().rev() {
            col -= crate::render::str_width(text) as i32;
            cv.text(col, y0, text, *f, *b);
        }

        cv.text(x0, tag_row, &fit("", rw as usize), MUTED, TAG_BG);
        if d == 0 {
            // Nobody home: one dark desk.
            let dx = x0 + rw / 2 - 5 * s;
            cv.rect(dx, b + 9 * s, 9 * s, s, Rgb::hex(0x4a5470));
            cv.rect(dx, b + 10 * s, 9 * s, 2 * s, Rgb::hex(0x232a40));
            let msg = "- no knights -";
            let pad = ((rw - crate::render::str_width(msg) as i32) / 2).max(0) as usize;
            cv.text(
                x0,
                tag_row,
                &fit(&format!("{}{msg}", " ".repeat(pad)), rw as usize),
                Rgb::hex(0x4a5576),
                TAG_BG,
            );
            return;
        }
        for (i, k) in ks.iter().take(MAX_DESKS).enumerate() {
            let sx = x0 + 1 + i as i32 * slot_w;
            self.draw_desk_knight(w, cv, k, (sx, slot_w), b, s, tag_row);
            // A desk waiting for an answer gets a blinking outline.
            if let Some(c) = lights[i].outline {
                let (x1, x2, y1, y2) = (sx - 1, sx + slot_w - 1, pt, pb - 1);
                cv.rect(x1, y1, x2 - x1 + 1, 1, c);
                cv.rect(x1, y2, x2 - x1 + 1, 1, c);
                cv.rect(x1, y1, 1, y2 - y1 + 1, c);
                cv.rect(x2, y1, 1, y2 - y1 + 1, c);
            }
        }
    }

    /// A knight at its desk inside a room, `s` pixels per sprite pixel.
    #[allow(clippy::too_many_arguments)]
    fn draw_desk_knight(&self, w: &World, cv: &mut Canvas, k: &Knight, slot: (i32, i32), b: i32, s: i32, tag_row: i32) {
        let (sx, slot_w) = slot;
        let t = w.time;
        let kx = sx + 1 + s;
        let mut ky = b + 3 * s;
        let mut closed = false;
        if k.state == KnightState::Idle {
            ky -= (((t * 1.5) as i32 + k.color_idx as i32) % 2) * s;
            closed = true;
        }
        let pal = self.knight_pal(w, k);
        let opts = SpriteOpts {
            closed,
            back: k.state == KnightState::Unknown,
            bow: false,
        };
        cv.sprite_scaled(&KNIGHT_MINI, &pal, kx, ky, opts, s);
        cv.rect(kx - s, b + 9 * s, 9 * s, s, Rgb::hex(0x9aa6c4));
        cv.rect(kx - s, b + 10 * s, 9 * s, 2 * s, Rgb::hex(0x2f3a5c));
        // Desk lamps blink while the knight is busy and go dark at rest.
        let lamps_on = matches!(k.state, KnightState::Working | KnightState::Blocked | KnightState::Done);
        for i in 0..3 {
            let lit = lamps_on && ((t * 3.0 + i as f32 * 1.3 + sx as f32) as i32) % 3 != 0;
            let c = if lit { k.crystal } else { k.crystal.mix(BLACK, 0.75) };
            cv.rect(kx + s * (1 + 2 * i), b + 11 * s, s, s, c);
        }
        let g = ((t * 8.0) as i32 + k.color_idx as i32) % 2;
        match k.state {
            KnightState::Working => {
                cv.rect(kx + s, b + 9 * s - g * s, s, s, k.skin);
                cv.rect(kx + 5 * s, b + 9 * s - (1 - g) * s, s, s, k.skin);
            }
            KnightState::Blocked => {
                let wave = (t * 4.0) as i32 % 2;
                cv.rect(kx + 7 * s, ky + s - wave * s, s, s, k.skin);
                cv.rect(kx + 7 * s, ky + 2 * s, s, 3 * s, Rgb::hex(0x4a4f8c));
                cv.rect(kx + 8 * s, b, 3 * s, 5 * s, BUBBLE);
                if (t * 3.0) as i32 % 2 == 0 {
                    cv.rect(kx + 9 * s, b + s, s, 2 * s, RED);
                    cv.rect(kx + 9 * s, b + 4 * s, s, s, RED);
                }
            }
            KnightState::Done => {
                cv.rect(kx + 8 * s, b, 3 * s, 5 * s, BUBBLE);
                for (dx, dy) in [(8, 2), (9, 3), (10, 1)] {
                    cv.rect(kx + dx * s, b + dy * s, s, s, GREEN);
                }
            }
            KnightState::Idle => {
                let ang = t * 1.6 + k.color_idx as f32;
                let x = (kx + 3 * s) as f32 + ang.cos() * 5.0 * s as f32;
                let y = (ky + 4 * s) as f32 + ang.sin() * 3.0 * s as f32;
                cv.rect(x.round() as i32, y.round() as i32, s, s, k.crystal.mix(WHITE, 0.4));
            }
            KnightState::Unknown => {}
        }

        // Apprentices beside the desk: reporters first, then by arrival.
        let crew: Vec<&Apprentice> = w.crew(&k.pane_id).filter(|p| p.phase != Phase::Exit).collect();
        let mut order: Vec<&Apprentice> = crew.iter().copied().filter(|p| !p.active()).collect();
        let mut working: Vec<&Apprentice> = crew.iter().copied().filter(|p| p.active()).collect();
        working.sort_by_key(|p| p.id);
        order.extend(working);
        let ax = kx + 8 * s;
        let room_for = ((sx + slot_w - 1 - ax) / (4 * s)).clamp(0, 3) as usize;
        for (n, p) in order.iter().take(room_for).enumerate() {
            let x = ax + n as i32 * 4 * s;
            let back = !p.active();
            let bob = i32::from(!back && ((t * 4.0) as i64 + p.id as i64) % 2 == 1);
            let y = b + 7 * s - bob * s;
            let opts = SpriteOpts {
                back,
                ..Default::default()
            };
            cv.sprite_scaled(
                &APPRENTICE_MINI,
                &apprentice_pal(k.crystal, p.skin, p.hair),
                x,
                y,
                opts,
                s,
            );
            if back && (t * 4.0) as i32 % 2 == 1 {
                cv.rect(x + s, y - 2 * s, 2 * s, s, k.crystal);
            }
        }

        // Name tag, with the apprentices that did not fit.
        let extra = order.len().saturating_sub(room_for);
        let right = if extra > 0 { format!("+{extra} ") } else { String::new() };
        let width = (slot_w as usize).saturating_sub(right.len());
        let label = fit(&format!(" {} {}", k.state.glyph(), k.name), width) + &right;
        let (mut fg, mut bg) = (k.crystal, TAG_BG);
        match k.state {
            KnightState::Blocked => {
                fg = AMBER_INK;
                bg = if (t * 2.0) as i32 % 2 == 0 { AMBER } else { AMBER_DIM };
            }
            KnightState::Idle | KnightState::Unknown => fg = k.crystal.mix(TAG_BG, 0.35),
            _ => {}
        }
        cv.text(sx, tag_row, &label, fg, bg);
    }
}

/// How one desk's share of a room is lit.
struct Light {
    wall: Rgb,
    floor: (Rgb, Rgb),
    rail: Rgb,
    /// Star brightness in the window, 0..1.
    stars: f32,
    ceiling: Option<Rgb>,
    outline: Option<Rgb>,
}

/// Lighting for a knight's state; `None` is an empty room.
fn light(state: Option<KnightState>, t: f32) -> Light {
    let dark = Light {
        wall: Rgb::hex(0x0e1222),
        floor: (Rgb::hex(0x0b0e1b), Rgb::hex(0x0d111f)),
        rail: Rgb::hex(0x181e33),
        stars: 0.3,
        ceiling: None,
        outline: None,
    };
    match state {
        None | Some(KnightState::Idle) | Some(KnightState::Unknown) => dark,
        Some(KnightState::Working) => Light {
            wall: Rgb::hex(0x2b3764),
            floor: (Rgb::hex(0x222c4e), Rgb::hex(0x283358)),
            rail: Rgb::hex(0x5566a0),
            stars: 1.0,
            ceiling: Some(Rgb::hex(0xdfe9ff)),
            outline: None,
        },
        Some(KnightState::Done) => Light {
            wall: Rgb::hex(0x183a32),
            floor: (Rgb::hex(0x13302a), Rgb::hex(0x17362e)),
            rail: Rgb::hex(0x2f8a64),
            stars: 0.8,
            ceiling: Some(Rgb::hex(0x7dffb0)),
            outline: None,
        },
        Some(KnightState::Blocked) => {
            let on = (t * 2.0) as i32 % 2 == 0;
            let glow = if on { 1.0 } else { 0.55 };
            Light {
                wall: Rgb::hex(0x1a1408).mix(Rgb::hex(0x4a360e), glow),
                floor: (Rgb::hex(0x2a200b), Rgb::hex(0x32270e)),
                rail: Rgb::hex(0x9a7020),
                stars: 0.8,
                ceiling: Some(if on { AMBER } else { AMBER_DIM }),
                outline: Some(if on { AMBER } else { AMBER_DIM }),
            }
        }
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
