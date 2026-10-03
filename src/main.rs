//! Starkeep: herdr agents as knights on an orbital training ship.

mod anim;
mod demo;
mod herdr;
mod hooks;
mod model;
mod render;
mod spots;
mod sprites;
mod view;

use std::io;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton,
    MouseEventKind,
};
use crossterm::execute;
use ratatui::DefaultTerminal;

use model::World;
use render::Canvas;
use view::{Mode, View};

const FRAME: Duration = Duration::from_millis(1000 / 12);

pub enum Msg {
    Snapshot(herdr::Snapshot),
    Offline(String),
    Hook(hooks::HookEvent),
    Replay(Vec<hooks::HookEvent>),
}

struct Args {
    demo: Option<usize>,
    snapshot: Option<String>,
    cols: u16,
    rows: u16,
    seconds: f32,
    zoom: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        demo: None,
        snapshot: None,
        cols: 112,
        rows: 34,
        seconds: 3.0,
        zoom: false,
    };
    let mut it = std::env::args().skip(1).peekable();
    while let Some(arg) = it.next() {
        let mut num = |name: &str| -> Result<f32, String> {
            it.next()
                .and_then(|v| v.parse().ok())
                .ok_or(format!("{name} needs a number"))
        };
        match arg.as_str() {
            "--demo" => a.demo = Some(7),
            "--knights" => a.demo = Some(num("--knights")? as usize),
            "--snapshot" => a.snapshot = Some(it.next().ok_or("--snapshot needs a path")?),
            "--cols" => a.cols = num("--cols")? as u16,
            "--rows" => a.rows = num("--rows")? as u16,
            "--seconds" => a.seconds = num("--seconds")?,
            "--zoom" => a.zoom = true,
            "-h" | "--help" => {
                println!(
                    "starkeep [--demo] [--knights N]\n\
                     \n  --demo        simulated knights, no herdr needed\
                     \n  --knights N   demo with N knights\
                     \n  --snapshot F  headless: run for --seconds and write a PPM of the frame\
                     \n\nkeys: arrows select a space, enter zooms in / goes to the pane and closes, esc zooms out / closes, q quits\
                     \nhook events: {}",
                    hooks::events_path().display()
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(a)
}

fn start_sources(args: &Args) -> Receiver<Msg> {
    let (tx, rx) = mpsc::channel();
    match args.demo {
        Some(n) => demo::spawn(tx, n.max(1)),
        None => {
            herdr::spawn_poller(tx.clone());
            hooks::spawn_tail(tx);
        }
    }
    rx
}

fn drain(rx: &Receiver<Msg>, w: &mut World) {
    while let Ok(msg) = rx.try_recv() {
        match msg {
            Msg::Snapshot(s) => w.apply_snapshot(s),
            Msg::Offline(why) => w.set_offline(why),
            Msg::Hook(ev) => w.hook(ev, false),
            Msg::Replay(evs) => {
                for ev in evs {
                    w.hook(ev, true);
                }
            }
        }
    }
}

fn main() -> io::Result<()> {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("starkeep: {e}");
            std::process::exit(2);
        }
    };
    if let Some(path) = &args.snapshot {
        return snapshot(&args, path);
    }
    let rx = start_sources(&args);
    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    let res = run(&mut terminal, rx);
    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();
    res
}

fn run(terminal: &mut DefaultTerminal, rx: Receiver<Msg>) -> io::Result<()> {
    let mut world = World::new();
    let mut view = View::new();
    let mut canvas = Canvas::new();
    let mut last = Instant::now();
    let mut next_sweep = Instant::now();
    loop {
        drain(&rx, &mut world);
        let size = terminal.size()?;
        view.apply_layout(&mut world, size.width, size.height);
        let dt = last.elapsed().as_secs_f32().min(0.25);
        last = Instant::now();
        anim::update(&mut world, dt);
        view.tick(&mut world, dt);
        if next_sweep <= Instant::now() {
            world.sweep();
            next_sweep = Instant::now() + Duration::from_secs(5);
        }
        terminal.draw(|f| {
            let area = f.area();
            view.draw(&world, &mut canvas);
            canvas.blit(f.buffer_mut(), area);
        })?;

        let deadline = Instant::now() + FRAME;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            if !event::poll(left)? {
                break;
            }
            if handle(event::read()?, &mut view, &world) {
                return Ok(());
            }
            view.apply_layout(&mut world, size.width, size.height);
        }
    }
}

/// Returns true to quit.
fn handle(ev: Event, view: &mut View, w: &World) -> bool {
    match ev {
        Event::Key(k) if k.kind != KeyEventKind::Release => {
            let zoomed = view.layout.mode == Mode::Zoom;
            match k.code {
                KeyCode::Char('q') => return true,
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => return true,
                KeyCode::Left | KeyCode::Char('h') => view.move_cursor(w, -1, 0),
                KeyCode::Right | KeyCode::Char('l') => view.move_cursor(w, 1, 0),
                KeyCode::Up | KeyCode::Char('k') => view.move_cursor(w, 0, -1),
                KeyCode::Down | KeyCode::Char('j') => view.move_cursor(w, 0, 1),
                KeyCode::Enter | KeyCode::Char(' ') if !zoomed => view.enter_selected(w),
                KeyCode::Enter => return jump(view),
                // Esc steps back out of a close-up, otherwise closes Starkeep.
                KeyCode::Esc if view.focus.is_some() => view.focus = None,
                KeyCode::Esc => return true,
                _ => {}
            }
        }
        Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => match view.layout.mode {
            Mode::Rooms => {
                if let Some((ws, knight)) = view.hit(w, m.column, m.row) {
                    view.enter_room(w, &ws, knight);
                }
            }
            Mode::Zoom => view.focus = None,
        },
        Event::Mouse(m) if m.kind == MouseEventKind::ScrollDown => view.move_cursor(w, 0, 1),
        Event::Mouse(m) if m.kind == MouseEventKind::ScrollUp => view.move_cursor(w, 0, -1),
        _ => {}
    }
    false
}

/// Go to the knight's pane and close Starkeep. Returns true to quit.
fn jump(view: &mut View) -> bool {
    let Some(pane) = view.focus.clone() else { return false };
    match herdr::focus_agent(&pane) {
        Ok(()) => true,
        Err(e) => {
            view.notice = Some((format!("cannot go to {pane}: {e}"), 3.0));
            false
        }
    }
}

/// Headless render for checking the art without a terminal.
fn snapshot(args: &Args, path: &str) -> io::Result<()> {
    let rx = start_sources(args);
    let mut world = World::new();
    let mut view = View::new();
    let mut canvas = Canvas::new();
    let step = 1.0 / 12.0;
    let mut t = 0.0;
    while t < args.seconds {
        drain(&rx, &mut world);
        if args.zoom && view.focus.is_none() {
            view.focus = world.knights.first().map(|k| k.pane_id.clone());
        }
        view.apply_layout(&mut world, args.cols, args.rows);
        anim::update(&mut world, step);
        view.tick(&mut world, step);
        std::thread::sleep(Duration::from_secs_f32(step));
        t += step;
    }
    if !world.online {
        eprintln!("starkeep: herdr {}", world.status);
    }
    eprintln!(
        "starkeep: {} knights, {} apprentices",
        world.knights.len(),
        world.apps.len()
    );
    view.draw(&world, &mut canvas);
    std::fs::write(path, canvas.ppm())
}
