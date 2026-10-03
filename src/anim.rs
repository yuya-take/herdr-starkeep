//! Walking, bowing, and the timers behind the ambience.

use crate::model::{Phase, Spot, World};
use crate::spots::{self, LANE};

const WALK_SPEED: f32 = 26.0;

/// Advance one step along the path. Returns true when the path is done.
/// Knights outside the large view finish their walks instantly.
fn step_path(w: &mut World, i: usize, dt: f32) -> bool {
    let in_large = w.knight(&w.apps[i].master).is_some_and(|k| k.bay.is_some());
    let p = &mut w.apps[i];
    let Some(&(tx, ty)) = p.path.front() else { return true };
    if !in_large {
        let &(ex, ey) = p.path.back().unwrap();
        p.x = ex;
        p.y = ey;
        p.path.clear();
        return true;
    }
    let sp = WALK_SPEED * dt;
    let (dx, dy) = (tx - p.x, ty - p.y);
    if dx.abs() > 0.01 {
        p.x += dx.signum() * sp.min(dx.abs());
        p.back = false;
    } else if dy.abs() > 0.01 {
        p.y += dy.signum() * sp.min(dy.abs());
        p.back = dy < 0.0;
    } else {
        p.path.pop_front();
    }
    p.walk_t += dt;
    p.path.is_empty()
}

pub fn update(w: &mut World, dt: f32) {
    w.time += dt;
    for k in w.knights.iter_mut() {
        k.flash = (k.flash - dt).max(0.0);
    }
    let scene_w = w.scene_w;
    let mut i = 0;
    while i < w.apps.len() {
        let phase = w.apps[i].phase;
        let mut remove = false;
        match phase {
            Phase::Enter | Phase::Move => {
                if step_path(w, i, dt) {
                    let p = &mut w.apps[i];
                    p.phase = Phase::Work;
                    p.back = false;
                    p.seated = matches!(p.spot, Spot::Floor(_));
                }
            }
            Phase::Work => {}
            Phase::Report => {
                if step_path(w, i, dt) {
                    let in_large = w.knight(&w.apps[i].master).is_some_and(|k| k.bay.is_some());
                    let p = &mut w.apps[i];
                    p.phase = Phase::Bow;
                    p.bow_t = if in_large { 1.6 } else { 2.4 };
                    p.back = true;
                }
            }
            Phase::Bow => {
                let p = &mut w.apps[i];
                p.bow_t -= dt;
                let mut deliver = false;
                if !p.delivered && p.bow_t < 1.0 {
                    p.delivered = true;
                    deliver = true;
                }
                if p.bow_t <= 0.0 {
                    p.phase = Phase::Exit;
                    p.back = false;
                    p.path = [(p.x, LANE), ((scene_w + 12) as f32, LANE)].into();
                }
                if deliver {
                    let master = w.apps[i].master.clone();
                    if let Some(k) = w.knight_mut(&master) {
                        k.flash = 1.6;
                    }
                }
            }
            Phase::Exit => remove = step_path(w, i, dt),
        }
        if remove {
            w.apps.remove(i);
        } else {
            i += 1;
        }
    }
    spots::promote_all(w);
}

/// A knight just appeared in the large view: put its apprentices where they
/// should be right now.
pub fn snap(w: &mut World, pane: &str, bay: usize) {
    w.apps.retain(|p| !(p.master == pane && p.phase == Phase::Exit));
    for p in w.apps.iter_mut().filter(|p| p.master == pane) {
        if p.active() {
            spots::place(p, bay);
        } else {
            let (rx, ry) = spots::report_xy(bay, p);
            p.x = rx;
            p.y = ry;
            p.path.clear();
            if p.phase == Phase::Report {
                p.phase = Phase::Bow;
                p.bow_t = 1.6;
            }
            p.back = true;
        }
    }
}

/// A knight left the large view: finish any walk instantly.
pub fn settle(w: &mut World, pane: &str) {
    for p in w.apps.iter_mut().filter(|p| p.master == pane) {
        if let Some(&(x, y)) = p.path.back() {
            p.x = x;
            p.y = y;
            p.path.clear();
        }
    }
}

pub struct Star {
    /// Position inside the window, both 0..1 so the field survives resizes.
    pub x: f32,
    pub y: f32,
    pub s: f32,
}

/// Star field for one window. Speed rises with the number of working knights.
pub struct Starfield {
    pub stars: Vec<Star>,
}

impl Starfield {
    pub fn new(w: &mut World, n: usize) -> Starfield {
        let stars = (0..n)
            .map(|_| Star {
                x: w.rng.f(),
                y: w.rng.f(),
                s: w.rng.range(0.5, 1.6),
            })
            .collect();
        Starfield { stars }
    }

    pub fn advance(&mut self, rng: &mut crate::model::Rng, dt: f32, warp: f32, width: f32) {
        for s in self.stars.iter_mut() {
            s.x -= s.s * warp * dt * 26.0 / width.max(1.0);
            if s.x < 0.0 {
                s.x += 1.0;
                s.y = rng.f();
            }
        }
    }
}
