//! Where apprentices stand around their knight, and how they move up.
//!
//! Each knight has 2 seats beside the desk, 3 spots on the floor in front of
//! it, and an unlimited "offdeck" that is only shown as a count. Earlier
//! apprentices always sit closer to the knight.

use std::collections::VecDeque;

use crate::model::{Apprentice, Phase, Spot, World};

/// y of the walkway apprentices use to enter, leave and change spots.
pub const LANE: f32 = 51.0;
pub const SEAT_Y: f32 = 39.0;
pub const FLOOR_Y: f32 = 49.0;
pub const REPORT_Y: f32 = 45.0;
pub const BAY_W: i32 = 37;

pub struct Station {
    pub sx: i32,
    pub desk_x: i32,
    pub kx: i32,
}

pub fn station(bay: usize) -> Station {
    let sx = 1 + bay as i32 * BAY_W;
    Station {
        sx,
        desk_x: sx + 9,
        kx: sx + 12,
    }
}

pub fn spot_xy(bay: usize, spot: Spot) -> (f32, f32) {
    let sx = station(bay).sx as f32;
    match spot {
        Spot::Seat(0) => (sx, SEAT_Y),
        Spot::Seat(_) => (sx + 28.0, SEAT_Y),
        Spot::Floor(i) => (sx + 4.0 + i as f32 * 10.0, FLOOR_Y),
        Spot::Offdeck => (sx, LANE),
    }
}

pub fn report_xy(bay: usize, p: &Apprentice) -> (f32, f32) {
    let st = station(bay);
    let x = match p.spot {
        Spot::Seat(0) => st.kx - 2,
        Spot::Seat(_) => st.kx + 5,
        _ => st.kx + 1 + ((p.id % 3) as i32 - 1) * 4,
    };
    (x as f32, REPORT_Y)
}

pub struct Occupancy {
    pub seat: [Option<u64>; 2],
    pub floor: [Option<u64>; 3],
}

pub fn occupancy(apps: &[Apprentice], master: &str) -> Occupancy {
    let mut o = Occupancy {
        seat: [None; 2],
        floor: [None; 3],
    };
    for p in apps.iter().filter(|p| p.master == master && p.active()) {
        match p.spot {
            Spot::Seat(i) => o.seat[i as usize] = Some(p.id),
            Spot::Floor(i) => o.floor[i as usize] = Some(p.id),
            Spot::Offdeck => {}
        }
    }
    o
}

pub fn assign_spot(apps: &[Apprentice], master: &str) -> Spot {
    let o = occupancy(apps, master);
    if let Some(i) = o.seat.iter().position(|s| s.is_none()) {
        return Spot::Seat(i as u8);
    }
    if let Some(i) = o.floor.iter().position(|s| s.is_none()) {
        return Spot::Floor(i as u8);
    }
    Spot::Offdeck
}

pub fn route_to(p: &mut Apprentice, bay: usize, spot: Spot) {
    let (tx, ty) = spot_xy(bay, spot);
    p.path = VecDeque::from([(p.x, LANE), (tx, LANE), (tx, ty)]);
    p.seated = false;
}

/// Jump straight to the current target, as if the walk had already happened.
pub fn place(p: &mut Apprentice, bay: usize) {
    if p.spot != Spot::Offdeck {
        let (x, y) = spot_xy(bay, p.spot);
        p.x = x;
        p.y = y;
    }
    p.path.clear();
    p.phase = Phase::Work;
    p.seated = matches!(p.spot, Spot::Floor(_));
    p.back = false;
}

fn relocate(p: &mut Apprentice, bay: usize, spot: Spot, scene_w: i32) {
    if p.spot == Spot::Offdeck {
        p.x = (scene_w + 1) as f32;
        p.y = LANE;
    }
    p.spot = spot;
    p.phase = Phase::Move;
    route_to(p, bay, spot);
}

/// Fill freed seats from the floor, and freed floor spots from offdeck.
pub fn promote_all(w: &mut World) {
    let masters: Vec<(String, usize)> = w
        .knights
        .iter()
        .map(|k| (k.pane_id.clone(), k.bay.unwrap_or(0)))
        .collect();
    for (master, bay) in masters {
        promote(w, &master, bay);
    }
}

fn promote(w: &mut World, master: &str, bay: usize) {
    let scene_w = w.scene_w;
    let waiting = |w: &World, from: &[Spot]| -> Option<usize> {
        w.apps
            .iter()
            .enumerate()
            .filter(|(_, p)| p.master == master && p.phase == Phase::Work && from.iter().any(|f| same_kind(*f, p.spot)))
            .min_by_key(|(_, p)| (rank(p.spot), p.id))
            .map(|(i, _)| i)
    };
    for i in 0..2u8 {
        if occupancy(&w.apps, master).seat[i as usize].is_some() {
            continue;
        }
        if let Some(idx) = waiting(w, &[Spot::Floor(0), Spot::Offdeck]) {
            relocate(&mut w.apps[idx], bay, Spot::Seat(i), scene_w);
        }
    }
    for i in 0..3u8 {
        if occupancy(&w.apps, master).floor[i as usize].is_some() {
            continue;
        }
        if let Some(idx) = waiting(w, &[Spot::Offdeck]) {
            relocate(&mut w.apps[idx], bay, Spot::Floor(i), scene_w);
        }
    }
}

fn same_kind(a: Spot, b: Spot) -> bool {
    std::mem::discriminant(&a) == std::mem::discriminant(&b)
}

/// Floor apprentices move up before offdeck ones.
fn rank(s: Spot) -> u8 {
    match s {
        Spot::Seat(_) => 0,
        Spot::Floor(_) => 1,
        Spot::Offdeck => 2,
    }
}

/// The subagent finished: walk to the knight to hand over the result.
pub fn finish_work(w: &mut World, id: u64) {
    let scene_w = w.scene_w;
    let Some(pi) = w.apps.iter().position(|p| p.id == id) else {
        return;
    };
    let bay = w.knight(&w.apps[pi].master).and_then(|k| k.bay).unwrap_or(0);
    let p = &mut w.apps[pi];
    if p.spot == Spot::Offdeck {
        p.x = (scene_w + 1) as f32;
        p.y = LANE;
    } else if p.seated {
        p.y = FLOOR_Y;
    }
    p.phase = Phase::Report;
    p.seated = false;
    let (rx, ry) = report_xy(bay, p);
    p.path = VecDeque::from([(p.x, LANE), (rx, LANE), (rx, ry)]);
}
