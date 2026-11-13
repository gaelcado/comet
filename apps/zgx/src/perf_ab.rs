//! Perf A/B harness (not committed): the same scenes on two builds, headless,
//! drawn by zgx's GPU renderer offscreen at a Retina scale, frame by frame on
//! the test's 60 Hz clock. A frame is delivered only when the app asks for
//! one (an animation) or input just came, as a window does.
//!
//!   PERF_OUT=<file.jsonl> PERF_SCENES=a,b PERF_REPEATS=5 \
//!     cargo test --release --bin zeron-zgx perf_ab -- --ignored --nocapture --test-threads=1

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use zgx::input::{InputEvent, PointerButton};
use zgx_gpu::GpuRenderer;
use zgx_script::{ScriptHost, Value, testing::TestApp};

const W: f32 = 1440.;
const H: f32 = 900.;
const SCALE: f32 = 2.;

fn ui() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zgx-perf-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn thread_cpu() -> Duration {
    let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: fills a timespec for this thread's CPU clock.
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) };
    Duration::new(t.tv_sec as u64, t.tv_nsec as u32)
}

fn process_cpu() -> Duration {
    // SAFETY: fills a rusage for this process.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    let t = |v: libc::timeval| Duration::new(v.tv_sec as u64, v.tv_usec as u32 * 1000);
    t(usage.ru_utime) + t(usage.ru_stime)
}

fn rss_kb() -> (u64, u64) {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let get = |k: &str| {
        s.lines()
            .find(|l| l.starts_with(k))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };
    (get("VmRSS:"), get("VmHWM:"))
}

struct Bench {
    app: TestApp,
    gpu: GpuRenderer,
    // Per delivered frame: main-thread CPU µs, wall µs, drawn.
    frames: Vec<(f64, f64, bool)>,
    recording: bool,
    skipped: usize,
    // Time the scene covers (test clock), in frames.
    ticks: usize,
    all: usize,
    proc_us: f64,
    idle_us: f64,
    paced: Instant,
    poke: bool,
}

impl Bench {
    fn mount(fixture: &str, storage: &Path, w: f32, h: f32) -> (Self, f64, f64) {
        let path = ui().join(fixture);
        let storage = storage.to_path_buf();
        let t0 = Instant::now();
        let c0 = thread_cpu();
        let app = TestApp::new(move |runtime| ScriptHost::new(runtime, path).storage(storage));
        let mut gpu = GpuRenderer::new((w * SCALE) as u32, (h * SCALE) as u32).expect("a GPU");
        gpu.set_scale_factor(SCALE);
        for font in crate::fonts() {
            font.expect("the app's fonts").install(&mut gpu.text_system().borrow_mut());
        }
        gpu.install_text(&mut app.ui.scene.borrow_mut());
        app.ui.refresh_text_geometry();
        app.ui.scene.borrow_mut().resize(w, h);
        let mut b = Bench { app, gpu, frames: Vec::new(), recording: false, skipped: 0, ticks: 0, all: 0, proc_us: 0., idle_us: 0., paced: Instant::now(), poke: true };
        // First frame drawn.
        b.app.frame();
        let report = b.app.ui.scene.borrow_mut().flush();
        b.gpu.render(&b.app.ui.scene.borrow(), &report.damage).expect("a frame drawn");
        let first_wall = t0.elapsed().as_secs_f64() * 1e3;
        let first_cpu = (thread_cpu() - c0).as_secs_f64() * 1e3;
        assert_eq!(b.app.error(), None, "the fixture failed");
        (b, first_wall, first_cpu)
    }

    /// One 60 Hz tick: a frame if one is wanted (or input came), else time passes.
    fn tick(&mut self) {
        let p = if self.recording { Some(process_cpu()) } else { None };
        self.tick_inner();
        if let Some(p) = p {
            self.proc_us += (process_cpu() - p).as_secs_f64() * 1e6;
        }
        // Paced at 60 Hz in real time, as a display: what reads the wall
        // clock (`ui.now()`: the presence's beat) sees time pass.
        let next = self.paced + Duration::from_secs_f64(1. / 60.);
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
            self.paced = next;
        } else {
            self.paced = now;
        }
    }
    fn tick_inner(&mut self) {
        self.all += 1;
        if self.recording {
            self.ticks += 1;
        }
        let t = Instant::now();
        let c = thread_cpu();
        // A display frame when one is asked for (an animation) or input came;
        // else time passes, timers fire, and what they changed is laid out
        // and drawn, as a window redraws on a state change.
        // (Every frame delivered: a timer's step sets an element's props, which
        // a window draws on its next refresh; headless, only a frame does.)
        let delivered = true;
        if delivered {
            self.poke = false;
            self.app.frame();
        } else {
            self.app.skip_frames(1);
            self.app.settle();
            self.app.ui.prepare_frame();
            self.app.settle();
        }
        let report = self.app.ui.scene.borrow_mut().flush();
        let drawn = !report.damage.is_empty();
        if drawn {
            self.gpu.render(&self.app.ui.scene.borrow(), &report.damage).expect("a frame drawn");
        }
        let cpu = (thread_cpu() - c).as_secs_f64() * 1e6;
        let wall = t.elapsed().as_secs_f64() * 1e6;
        if !delivered && !drawn {
            if self.recording {
                self.skipped += 1;
                self.idle_us += cpu;
            }
            return;
        }
        if self.recording {
            self.frames.push((cpu, wall, drawn));
        }
    }
    fn ticks(&mut self, n: usize) {
        for _ in 0..n {
            self.tick();
        }
    }
    /// Unrecorded frames until nothing moves (at most `n`).
    fn settle(&mut self, n: usize) {
        let was = self.recording;
        self.recording = false;
        for _ in 0..n {
            self.poke = true;
            self.tick();
            if !self.app.clock.wants_frame() {
                break;
            }
        }
        self.recording = was;
    }
    fn command(&mut self, c: &str) {
        assert!(self.app.host.run_command(c), "command {c}");
        self.poke = true;
    }
    fn click(&mut self, sel: &str) {
        let b = self.app.bounds(sel);
        let (x, y) = (b.x + b.width / 2., b.y + b.height / 2.);
        let button = PointerButton::Primary;
        self.app.ui.dispatch(InputEvent::PointerMove { x, y });
        self.app.ui.dispatch(InputEvent::PointerDown { x, y, button });
        self.app.ui.dispatch(InputEvent::PointerUp { x, y, button });
        self.app.ui.input.forget_clicks();
        self.app.settle();
        self.poke = true;
    }
    fn state(&mut self, name: &str, json: &str) {
        self.app.host.set_state(name, Value::from_json(json).expect("json")).expect("state");
        self.poke = true;
    }
    /// A press at `sel`'s middle dragged `dx` across `n` frames, then released.
    fn drag(&mut self, sel: &str, dx: f32, n: usize) {
        let b = self.app.bounds(sel);
        let (x, y) = (b.x + b.width / 2., b.y + b.height / 2.);
        let button = PointerButton::Primary;
        self.app.ui.dispatch(InputEvent::PointerMove { x, y });
        self.app.ui.dispatch(InputEvent::PointerDown { x, y, button });
        self.poke = true;
        self.tick();
        for i in 1..=n {
            let px = x + dx * i as f32 / n as f32;
            self.app.ui.dispatch(InputEvent::PointerMove { x: px, y });
            self.poke = true;
            self.tick();
        }
        self.app.ui.dispatch(InputEvent::PointerUp { x: x + dx, y, button });
        self.app.ui.input.forget_clicks();
        self.poke = true;
    }
    fn wheel(&mut self, sel: &str, dy: f32, n: usize) {
        let b = self.app.bounds(sel);
        let (x, y) = (b.x + b.width / 2., b.y + b.height / 2.);
        self.app.ui.dispatch(InputEvent::PointerMove { x, y });
        for _ in 0..n {
            self.app.ui.dispatch(InputEvent::Scroll { x, y, delta_x: 0., delta_y: dy });
            self.poke = true;
            self.tick();
        }
    }
    fn resize(&mut self, w: f32) {
        self.app.ui.scene.borrow_mut().resize(w, H);
        self.gpu.resize((w * SCALE) as u32, (H * SCALE) as u32);
        self.poke = true;
    }
    /// Real time for pictures to decode off the thread, frames as asked.
    fn wait_ms(&mut self, ms: u64) {
        let was = self.recording;
        self.recording = false;
        let end = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < end {
            self.poke = true;
            self.tick();
            std::thread::sleep(Duration::from_millis(5));
        }
        self.recording = was;
    }
}

fn shell(storage: &Path) -> (Bench, f64, f64) {
    let (mut b, w, c) = Bench::mount("shell_fixture.luau", storage, W, H);
    // (Experiments: states set first, `PERF_STATES='{"name": value}'`.)
    if let Ok(states) = std::env::var("PERF_STATES") {
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&states).expect("PERF_STATES");
        for (k, v) in map {
            b.state(&k, &v.to_string());
        }
    }
    b.settle(400);
    b.click("#shell-sidebar >> role:button \"Fix the build\"");
    b.settle(400);
    (b, w, c)
}

const TRIO: &str = r#"[{"kind":"row","children":[{"kind":"leaf","id":"a1"},{"kind":"leaf","id":"a2"},{"kind":"leaf","id":"a3"}]}]"#;
const ONE: &str = r#"[{"kind":"row","children":[{"kind":"leaf","id":"a1"}]}]"#;
const SPLIT: &str = "screens/shell/split.luau::inset_split_trees";

fn scene(name: &str, storage: &Path) -> Option<(Bench, f64, f64)> {
    let r = match name {
        "idle" => {
            let (mut b, w, c) = shell(storage);
            b.recording = true;
            b.ticks(600);
            (b, w, c)
        }
        "work1" | "work_shell" => {
            let (mut b, w, c) = shell(storage);
            b.command("work");
            b.settle(30);
            b.recording = true;
            b.ticks(600);
            (b, w, c)
        }
        "presence1" | "presence12" => {
            let (mut b, w, c) = Bench::mount("presence_fixture.luau", storage, W, H);
            if name == "presence1" {
                b.state("presence_rows", "1");
            }
            b.settle(30);
            b.recording = true;
            b.ticks(600);
            (b, w, c)
        }
        "panel" => {
            let (mut b, w, c) = shell(storage);
            b.recording = true;
            for _ in 0..3 {
                b.command("shell.toggle_files");
                b.ticks(45);
                b.command("shell.toggle_files");
                b.ticks(45);
            }
            b.command("shell.toggle_files");
            b.ticks(45);
            for _ in 0..2 {
                b.click("#panel-expand");
                b.ticks(45);
                b.click("#panel-expand");
                b.ticks(45);
            }
            (b, w, c)
        }
        "sidebar" => {
            let (mut b, w, c) = shell(storage);
            b.recording = true;
            for _ in 0..3 {
                b.command("shell.toggle_sidebar");
                b.ticks(40);
                b.command("shell.toggle_sidebar");
                b.ticks(40);
            }
            (b, w, c)
        }
        "split" => {
            let (mut b, w, c) = shell(storage);
            b.command("shell.toggle_files");
            b.settle(120);
            b.recording = true;
            for _ in 0..3 {
                b.state(SPLIT, TRIO);
                b.ticks(60);
                b.state(SPLIT, ONE);
                b.ticks(60);
            }
            (b, w, c)
        }
        "cmd_e" => {
            let (mut b, w, c) = shell(storage);
            b.command("shell.toggle_files");
            b.settle(120);
            b.recording = true;
            for _ in 0..4 {
                b.command("shell.toggle_files_tab");
                b.ticks(45);
            }
            (b, w, c)
        }
        "drag_top" => {
            let (mut b, w, c) = shell(storage);
            for s in ["Write the docs", "Tidy the tests", "Fix the build"] {
                b.click(&format!("#shell-sidebar >> role:button \"{s}\""));
                b.settle(120);
            }
            b.recording = true;
            for _ in 0..3 {
                b.drag("#session-tab-a1", 300., 40);
                b.ticks(40);
                b.drag("#session-tab-a1", -300., 40);
                b.ticks(40);
            }
            (b, w, c)
        }
        "drag_panel" => {
            let (mut b, w, c) = shell(storage);
            b.command("shell.toggle_files");
            b.settle(120);
            b.click("#panel-open-files");
            b.settle(120);
            b.command("shell.toggle_terminal");
            b.settle(120);
            b.recording = true;
            for _ in 0..3 {
                b.drag("#panel-tab-tab1", 160., 30);
                b.ticks(40);
                b.drag("#panel-tab-tab1", -160., 30);
                b.ticks(40);
            }
            (b, w, c)
        }
        n if n.starts_with("wall_") => {
            let effect = &n[5..];
            let (mut b, w, c) = shell(storage);
            b.state("shell_wallpaper", "\"image\"");
            b.state(
                "shell_wallpaper_image",
                r#"{"path":"assets/art/plate-1-night.jpg","name":"night","w":2100,"h":700,"x":0.5,"y":0.5,"zoom":1}"#,
            );
            b.state("shell_wallpaper_effect", &format!("\"{effect}\""));
            b.recording = true;
            b.command("shell.new_chat");
            b.ticks(10);
            b.recording = false;
            b.wait_ms(800);
            b.recording = true;
            b.ticks(90);
            for _ in 0..3 {
                b.click("#shell-sidebar >> role:button \"Fix the build\"");
                b.ticks(45);
                b.command("shell.new_chat");
                b.ticks(45);
            }
            (b, w, c)
        }
        "scroll" => {
            let (mut b, w, c) = Bench::mount("chat_fixture.luau", storage, W, H);
            b.command("go_s1");
            b.wait_ms(600);
            b.settle(200);
            b.recording = true;
            for _ in 0..2 {
                b.wheel("#chat-session", -60., 90);
                b.ticks(20);
                b.wheel("#chat-session", 60., 90);
                b.ticks(20);
            }
            (b, w, c)
        }
        "resize" | "resize_closed" => {
            let (mut b, w, c) = shell(storage);
            if name == "resize" {
                b.command("shell.toggle_files");
            }
            b.settle(120);
            b.recording = true;
            for _ in 0..2 {
                for i in 1..=44 {
                    b.resize(W - 10. * i as f32);
                    b.tick();
                }
                for i in (0..44).rev() {
                    b.resize(W - 10. * i as f32);
                    b.tick();
                }
            }
            b.ticks(30);
            (b, w, c)
        }
        _ => return None,
    };
    Some(r)
}

fn pct(v: &mut [f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let i = ((v.len() as f64 - 1.) * p).round() as usize;
    v[i]
}

#[test]
#[ignore = "a benchmark"]
fn perf_ab() {
    let out = std::env::var("PERF_OUT").expect("PERF_OUT");
    let repeats: usize = std::env::var("PERF_REPEATS").ok().and_then(|v| v.parse().ok()).unwrap_or(5);
    let scenes = std::env::var("PERF_SCENES").expect("PERF_SCENES");
    let side = std::env::var("PERF_SIDE").unwrap_or_default();
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&out).unwrap();
    for name in scenes.split(',') {
        for rep in 0..repeats {
            let storage = scratch(&format!("{name}-{rep}"));
            let p0 = process_cpu();
            let t0 = Instant::now();
            let Some((mut b, first_wall, first_cpu)) = scene(name, &storage) else {
                panic!("no scene {name}");
            };
            let _ = (p0, t0);
            let total_cpu: f64 = b.frames.iter().map(|f| f.0).sum::<f64>() + b.idle_us;
            // Per frame: the frames that drew something.
            let mut cpu: Vec<f64> = b.frames.iter().filter(|f| f.2).map(|f| f.0).collect();
            let mut wall: Vec<f64> = b.frames.iter().filter(|f| f.2).map(|f| f.1).collect();
            let drawn = cpu.len();
            let n = b.frames.len();
            let line = serde_json::json!({
                "side": side, "scene": name, "rep": rep,
                "frames": n, "drawn": drawn, "skipped": b.skipped, "ticks": b.ticks,
                "cpu_med_us": pct(&mut cpu, 0.5), "cpu_p95_us": pct(&mut cpu, 0.95),
                "wall_med_us": pct(&mut wall, 0.5), "wall_p95_us": pct(&mut wall, 0.95),
                "cpu_total_ms": total_cpu / 1e3,
                // Main-thread CPU over the scene's own (test-clock) time.
                "cpu_pct": total_cpu / 1e6 / (b.ticks.max(1) as f64 / 60.) * 100.,
                "proc_cpu_pct": b.proc_us / 1e6 / (b.ticks.max(1) as f64 / 60.) * 100.,
                "proc_cpu_ms": b.proc_us / 1e3,
                "first_frame_wall_ms": first_wall, "first_frame_cpu_ms": first_cpu,
                "error": b.app.error(),
            });
            writeln!(file, "{line}").unwrap();
            println!("{line}");
            drop(b);
            let _ = std::fs::remove_dir_all(&storage);
        }
    }
}

/// A two-minute session (7200 frames of the scenes' interactions), then the
/// process's resident memory.
#[test]
#[ignore = "a benchmark"]
fn perf_memory() {
    let out = std::env::var("PERF_OUT").expect("PERF_OUT");
    let side = std::env::var("PERF_SIDE").unwrap_or_default();
    let storage = scratch("mem");
    let (mut b, _, _) = shell(&storage);
    let (rss0, _) = rss_kb();
    let mut steps = 0usize;
    while b.all < 7200 {
        match steps % 6 {
            0 => b.command("shell.toggle_files"),
            1 => b.command("shell.toggle_sidebar"),
            2 => b.command("shell.new_chat"),
            3 => b.command("shell.next_session"),
            4 => b.command("shell.next_session"),
            _ => b.command("work"),
        }
        b.ticks(60);
        steps += 1;
    }
    b.settle(200);
    let (rss, hwm) = rss_kb();
    let line = serde_json::json!({ "side": side, "scene": "memory_2min", "rss_start_kb": rss0, "rss_end_kb": rss, "hwm_kb": hwm, "ticks": b.all, "error": b.app.error() });
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&out).unwrap();
    writeln!(file, "{line}").unwrap();
    println!("{line}");
}
