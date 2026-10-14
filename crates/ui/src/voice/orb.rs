//! The call's orb: a sphere of glass with a fluid inside it.
//!
//! The glass is the optical envelope of the Glass Orb reference
//! (github.com/gaelcado/glass-orb): the body is transparent where the fluid
//! is dark, the fluid fades towards the limb, a Fresnel atmosphere in the
//! fluid's colours gathers there, and a thin rim of light runs round the
//! silhouette, brighter towards the upper left. Nothing is cast around it:
//! no glow, no shadow.
//!
//! The fluid is simulated, not drawn from a recipe ([`Fluid`]): a slow,
//! incompressible flow in a round vessel, carrying dye, seen through the
//! sphere as through a lens. The two voices stir it band by band ([`Voice`]):
//! the user's comes in from the glass and Codex's goes out from the centre,
//! each band from its own place and in its own manner, so what is said
//! shapes the flow, and what is said also shows at once, before the fluid
//! has moved ([`Touch`]). What the call is doing is how the fluid is stirred
//! and what colour it is ([`Mood`]), never an icon: every state is a place in the same
//! material, and the material glides between them, so nothing is swapped. A
//! state also has to last before the orb follows it ([`State::hold`]), so the
//! pauses inside a sentence do not flick it between speaking and listening.
//!
//! GPUI has no per-element shader, so each frame is computed here at a low
//! resolution, where a soft fluid loses nothing, and painted as an image; the
//! rim, which has to be crisp, is computed once at the display's own
//! resolution and laid over it.
use std::{
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use gpui::{Bounds, Corners, Hsla, ParentElement, RenderImage, Rgba, Styled, Window, div, px};

pub(crate) use super::hearing::{BANDS, Bands};
use crate::theme::Theme;

/// The disc's diameter while the call connects, as a share of its own, and
/// how much of it shows.
const WAITING_SIZE: f32 = 0.92;
const WAITING_PRESENCE: f32 = 0.6;
/// Seconds for the fluid's character to cover most of the way to a new
/// state's: the glide between states.
const GLIDE: f32 = 0.38;
/// The longest one frame counts for, in the glide and in the fluid.
const STEP: f32 = 0.1;
/// Seconds per breath of a call busy without a sound.
const BREATH_PERIOD: f32 = 2.4;
/// The lattice the fluid's noise is drawn from.
const LATTICE: usize = 256;
/// The reference's rim: its direction (up and to the left), its width in
/// device pixels, and its strength on dark and on light surfaces. On a
/// light surface the glass also shades its own edge, in this ink.
const RIM_TOWARDS: (f32, f32) = (-0.65, -0.76);
const RIM_BAND: (f32, f32) = (0.75, 2.75);
// The reference's own strength on a dark surface (its `glassRim`, 0.5):
// nearer full white it read as a ring drawn round the orb.
const RIM_STRENGTH: (f32, f32) = (0.5, 0.7);
const RIM_SHADE: f32 = 0.1;
const RIM_SHADE_INK: f32 = 0.2;
/// On a light surface: how dark the hairline on the silhouette is towards
/// the light and how much darker away from it.
const LIGHT_EDGE: (f32, f32) = (0.14, 0.22);
/// On a light surface, the light's bloom on the sphere's shoulder: how far
/// from the centre, how wide and how strong. A dark surface has none: on an
/// empty glass it is a smudge.
const BLOOM: (f32, f32, f32) = (0.5, 0.46, 0.12);
/// The sheen inside the edge: where (as a share of the radius), how wide,
/// and how strong towards the light and away from it.
const SHEEN: (f32, f32, f32, f32) = (0.94, 0.04, 0.24, 0.05);
/// A band that wide suits a large orb and is a white collar on a small one:
/// under the first size (in device pixels) the band is this fraction as wide
/// and as strong, and from the second size up it is whole.
const RIM_FINE: (f32, f32, f32) = (48.0, 240.0, 0.45);
/// The fluid's body is no lighter than the accent's own fill, and as dark
/// as this share of it (of its light, so the shade is the same colour and
/// not a more vivid one).
const BODY_SHADE: f32 = 0.62;
/// How much of the accent's light tone is in the light the user's voice
/// brings: the rest is the surface's ink.
const USER_TINT: f32 = 0.35;
/// On a light surface: how much of the body's palest tone is white, how
/// dense the fluid shows, and how far it darkens towards its own limb (a
/// ball of it has to be shaded to be a ball: light cannot add there).
const LIGHT_TINT: f32 = 0.45;
const LIGHT_DENSITY: f32 = 1.1;
const LIGHT_LIMB: f32 = 0.3;
/// The fluid's ball inside the glass, as a share of the glass's radius, and
/// how far out from its own centre it starts to fade: its edge is soft, and
/// the glass is clear beyond it.
const CORE: f32 = 0.985;
const CORE_EDGE: f32 = 0.84;
/// The dye below which the fluid is clear water, and at which it is whole.
const CLEAR: (f32, f32) = (0.24, 0.95);
/// How strong the reference's atmosphere is at the limb, and on a dark
/// surface how much of it is left on the lit side and added opposite it.
const HAZE: f32 = 0.15;
const CRESCENT: (f32, f32) = (0.15, 1.6);
/// The fluid is a little sandy: its grain is this strong, drawn this many
/// times finer than the fluid itself is computed (as far as the disc has the
/// pixels for it), and this much of it shimmers while a voice sounds.
const GRAIN: f32 = 0.26;
const GRAIN_FINER: u32 = 2;
const GRAIN_SHIMMER: f32 = 0.45;
/// The reference's Fresnel exponent.
const FRESNEL: f32 = 2.8;
/// The fluid's grid: cells across for an orb drawn at least this many
/// pixels across, and for a smaller one.
const GRID: (u32, usize, usize) = (64, 48, 24);

/// What the call is doing, as far as the orb shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum State {
    /// Connecting or ending.
    Loading,
    /// The user has the floor.
    Listening,
    /// Codex speaks.
    Speaking,
    /// Codex works.
    Thinking,
    /// Codex looks something up.
    Searching,
    /// Codex waits on an answer.
    Asking,
    /// The microphone is muted.
    Muted,
}

impl State {
    /// The orb's state for a call in `state`; `searching` is whether the
    /// work Codex is doing is a lookup.
    pub(crate) fn of(state: Option<super::VoiceState>, searching: bool) -> Self {
        use super::VoiceState as Voice;
        match state {
            None | Some(Voice::Connecting | Voice::Ending) => Self::Loading,
            Some(Voice::Listening) => Self::Listening,
            Some(Voice::Speaking) => Self::Speaking,
            Some(Voice::Working) if searching => Self::Searching,
            Some(Voice::Working) => Self::Thinking,
            Some(Voice::Asking) => Self::Asking,
            Some(Voice::Muted) => Self::Muted,
        }
    }

    /// How long a state must last before the orb leaves `self` for `to`.
    /// Speech has pauses in it, so the orb stays with a speaker through
    /// them; a speaker starting is followed at once, bar a blip; what the
    /// user did (mute) and the call's own phases are immediate.
    fn hold(self, to: Self) -> Duration {
        Duration::from_millis(match (self, to) {
            (Self::Muted | Self::Loading, _) | (_, Self::Muted | Self::Loading) => 0,
            (Self::Speaking, _) => 900,
            (_, Self::Speaking) => 120,
            _ => 300,
        })
    }
}

/// What a state does to the fluid: how it shows, and what stirs it. Every
/// state stirs it its own way, so a change of state is a change of motion,
/// not only of colour.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Mood {
    /// How full the fluid shows at rest, and how far the voice lifts it.
    rest: f32,
    lift: f32,
    /// Whether its fullness breathes: the call is busy without a sound.
    breath: f32,
    /// The slow drift that keeps it alive.
    stir: f32,
    /// A turn about the centre: the fluid winds into a spiral.
    turn: f32,
    /// A stirrer circling near the glass: something being looked for.
    scan: f32,
    /// A puff from below, again and again: the call waiting on an answer.
    pulse: f32,
    /// A seed of light fed at the centre of an otherwise empty glass: the
    /// call being placed. The fluid is born of it once the call connects.
    seed: f32,
    /// How thick it is: how fast it comes to rest.
    drag: f32,
    /// The fluid's own tones: the two its body lies between, what Codex's
    /// voice lights it to, and the peak of that light.
    colours: [[f32; 3]; 4],
    /// The light the user's voice brings into it: the surface's own, not
    /// the accent, so the two voices are never mistaken for one another.
    user: [f32; 3],
    /// How much of the surface behind shows through a thin fluid: a light
    /// surface washes it out, so it is denser there.
    dense: f32,
    /// How far the fluid darkens towards its own limb.
    limb: f32,
    /// How far the glass's atmosphere gathers opposite the light.
    crescent: f32,
}

fn rgb(colour: Hsla) -> [f32; 3] {
    let Rgba { r, g, b, .. } = Rgba::from(colour);
    [r, g, b]
}

fn blend(from: [f32; 3], to: [f32; 3], amount: f32) -> [f32; 3] {
    std::array::from_fn(|i| from[i] + (to[i] - from[i]) * amount)
}

impl Mood {
    /// Tones of `base` and nothing else: two shades of it for the body,
    /// itself for what is lit, and itself lighter for the peaks. The hue is
    /// never turned.
    fn tones(base: Hsla) -> [[f32; 3]; 4] {
        let light = Hsla {
            l: (base.l + 0.12).min(0.86),
            ..base
        };
        let base = rgb(base);
        [
            base.map(|c| c * BODY_SHADE * BODY_SHADE),
            base.map(|c| c * BODY_SHADE),
            base,
            rgb(light),
        ]
    }

    /// The accent for a live call, whoever speaks; the warning colour when
    /// Codex needs the user; grey when the microphone is off or the call is
    /// not there yet.
    fn of(state: State, theme: &Theme) -> Self {
        // The accent as the theme authors it for its own mark: the fill its
        // accent controls carry, the accent, and its light tone. No hue is
        // derived here, so the fluid is the accent the rest of the app
        // shows. On a dark surface the fluid at rest is dark: its body lies
        // between that fill and a shade of it, the accent itself is what
        // Codex's voice lights, and the light tone is kept for the peaks. On
        // a light surface, where a dark veil reads as mud, it runs the other
        // way, as ink does: a light body, deepening where the voice is.
        let dark = theme.appearance.is_dark();
        let [light, mid, deep] = theme.glyph.rows().map(rgb);
        let shade = deep.map(|c| c * BODY_SHADE);
        let accent = if dark {
            [shade, deep, mid, light]
        } else {
            // A tint of the accent in clear glass, the accent where the
            // fluid is thick, and its fill where the voice is: never darker
            // than what the app's accent controls carry.
            [blend(light, [1.0; 3], LIGHT_TINT), light, mid, deep]
        };
        let neutral = [
            rgb(theme.text_faint),
            rgb(theme.text_muted),
            rgb(theme.text_muted),
            rgb(theme.text),
        ];
        let towards = |amount: f32| std::array::from_fn(|i| blend(accent[i], neutral[i], amount));
        // The user's light is the surface's own, not the accent: its ink.
        // On a light surface that is a soft graphite, since light cannot
        // show there.
        let clear = rgb(if dark { theme.text } else { theme.text_muted });
        let still = Self {
            rest: 0.5,
            // Whatever the call is doing, a voice shows.
            lift: 0.25,
            breath: 0.0,
            stir: 0.8,
            turn: 0.0,
            scan: 0.0,
            pulse: 0.0,
            seed: 0.0,
            drag: 0.4,
            colours: accent,
            user: blend(clear, light, USER_TINT),
            dense: if dark { 1.0 } else { LIGHT_DENSITY },
            limb: if dark { 0.0 } else { LIGHT_LIMB },
            crescent: if dark { 1.0 } else { 0.0 },
        };
        match state {
            // In the accent from the first frame: the wait is the orb
            // being born, not a placeholder for it.
            State::Loading => Self {
                rest: 0.55,
                lift: 0.0,
                breath: 0.25,
                stir: 0.5,
                turn: 0.7,
                seed: 1.0,
                drag: 0.5,
                ..still
            },
            State::Listening => Self {
                rest: 0.5,
                lift: 0.3,
                ..still
            },
            State::Speaking => Self {
                rest: 0.6,
                lift: 0.3,
                stir: 1.0,
                drag: 0.35,
                ..still
            },
            State::Thinking => Self {
                rest: 0.62,
                breath: 0.25,
                turn: 1.0,
                drag: 0.3,
                ..still
            },
            State::Searching => Self {
                rest: 0.66,
                breath: 0.15,
                turn: 0.3,
                scan: 1.0,
                drag: 0.3,
                ..still
            },
            State::Asking => Self {
                rest: 0.6,
                breath: 0.3,
                stir: 0.6,
                pulse: 1.0,
                drag: 0.5,
                colours: Self::tones(theme.warning),
                ..still
            },
            State::Muted => Self {
                rest: 0.3,
                stir: 0.3,
                drag: 2.5,
                colours: towards(1.0),
                ..still
            },
        }
    }

    fn towards(self, to: Self, amount: f32) -> Self {
        let mix = |from: f32, to: f32| from + (to - from) * amount;
        Self {
            rest: mix(self.rest, to.rest),
            lift: mix(self.lift, to.lift),
            breath: mix(self.breath, to.breath),
            stir: mix(self.stir, to.stir),
            turn: mix(self.turn, to.turn),
            scan: mix(self.scan, to.scan),
            pulse: mix(self.pulse, to.pulse),
            seed: mix(self.seed, to.seed),
            drag: mix(self.drag, to.drag),
            colours: std::array::from_fn(|i| blend(self.colours[i], to.colours[i], amount)),
            user: blend(self.user, to.user, amount),
            dense: mix(self.dense, to.dense),
            limb: mix(self.limb, to.limb),
            crescent: mix(self.crescent, to.crescent),
        }
    }
}

fn lattice() -> &'static [f32] {
    static LATTICE_VALUES: OnceLock<Vec<f32>> = OnceLock::new();
    LATTICE_VALUES.get_or_init(|| {
        let mut seed = 0x9e37_79b9_u32;
        (0..LATTICE * LATTICE)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed >> 8) as f32 / (1u32 << 24) as f32
            })
            .collect()
    })
}

fn smooth(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Soft value noise, 0–1, with features one unit across.
#[inline]
fn noise(table: &[f32], x: f32, y: f32) -> f32 {
    let (xf, yf) = (x.floor(), y.floor());
    let (fx, fy) = (x - xf, y - yf);
    let (fx, fy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (x0, y0) = (
        xf.rem_euclid(LATTICE as f32) as usize,
        yf.rem_euclid(LATTICE as f32) as usize,
    );
    let (x1, y1) = ((x0 + 1) % LATTICE, (y0 + 1) % LATTICE);
    let top = table[y0 * LATTICE + x0] + (table[y0 * LATTICE + x1] - table[y0 * LATTICE + x0]) * fx;
    let bottom =
        table[y1 * LATTICE + x0] + (table[y1 * LATTICE + x1] - table[y1 * LATTICE + x0]) * fx;
    top + (bottom - top) * fy
}

/// The fluid: an incompressible flow in a round vessel, on a square grid
/// over the vessel's bounding box (-1 to 1 each way), carrying three dyes.
///
/// Each step the flow is stirred (by a slow drift that keeps it alive, by a
/// turn about the centre when the call is working, and by the two voices,
/// band by band, out from the centre), has its small eddies put back (vorticity
/// confinement), carries itself along, thickens, comes to rest at the glass,
/// and is made incompressible again (pressure projection). The dyes ride it:
/// `dye` is how much fluid shows, `tone` which of its two colours, `heat`
/// what Codex's voice (or the call's own work) has just lit, and `cool` what
/// the user's has. So the fluid has inertia: a word leaves a plume that curls
/// into an eddy and takes seconds to settle.
struct Fluid {
    n: usize,
    u: Vec<f32>,
    v: Vec<f32>,
    dye: Vec<f32>,
    tone: Vec<f32>,
    heat: Vec<f32>,
    cool: Vec<f32>,
    /// Scratch, kept between steps.
    a: Vec<f32>,
    b: Vec<f32>,
    c: Vec<f32>,
    d: Vec<f32>,
    e: Vec<f32>,
    /// Per cell: 1 inside the vessel, and how free the flow is of the glass.
    inside: Vec<f32>,
    wall: Vec<f32>,
    time: f32,
    /// Each voice's bands, averaged: what is above the average is an onset.
    heard: [Bands; 2],
}

/// How hard each thing pushes, in vessel radii per second squared: the slow
/// drift, the turn about the centre, the circling stirrer, and the puff
/// from below.
const STIR: f32 = 0.045;
const TURN: f32 = 0.55;
const SCAN: f32 = 1.8;
const PULSE: f32 = 3.4;
/// Each voice pushes the fluid band by band, out from the centre, this hard.
const DRIVE: f32 = 6.4;
/// The two voices move the fluid alike; what tells them apart is the light
/// they bring into it: Codex's the accent (`heat`), the user's the surface's
/// own ink (`cool`). The user's bands push from the places between Codex's
/// (this fraction of a turn on), so two voices at once do not lie on one
/// another.
const USER_PLACE: f32 = 0.1;
/// How much of the user's light shows, next to the rest of the fluid.
const USER_SHOWS: f32 = 0.5;
/// …by how much of each band is its level and how much its onsets (a
/// syllable is a kick, not only a height), told over this many seconds.
const VOICE: (f32, f32) = (0.6, 2.2);
const ONSET: f32 = 0.15;
/// What each band is like, lowest first: how wide its push is (a low band
/// is broad and heavy, a high one narrow and quick), how hard, how much of
/// it goes round rather than straight, so the high bands spin small eddies
/// where the low ones move the whole body, and what it feeds the fluid: a
/// vowel thickens it, a consonant lights it.
const BAND_FOOT: [f32; BANDS] = [0.36, 0.27, 0.19, 0.13, 0.1];
const BAND_PUSH: [f32; BANDS] = [0.4, 0.65, 1.0, 1.3, 1.5];
const BAND_CURL: [f32; BANDS] = [0.0, 0.15, 0.4, 0.8, 1.1];
const BAND_BODY: [f32; BANDS] = [0.7, 0.6, 0.45, 0.25, 0.15];
const BAND_GLOW: [f32; BANDS] = [0.15, 0.3, 0.5, 0.75, 0.9];
/// How strongly small eddies are put back, and how much more while the
/// high bands sound.
const EDDIES: (f32, f32) = (0.04, 0.1);
/// Rounds of the pressure solve.
const PRESSURE_ROUNDS: usize = 16;
/// How fast what pushes the fluid also feeds it dye and heat, and how fast
/// the dye returns to its mist, its tone to its own drift, and the heat
/// fades, per second.
const FEED: (f32, f32) = (1.0, 3.2);
const SETTLE: (f32, f32, f32) = (0.45, 0.06, 0.7);
/// How wide a push is, and how far from the glass the flow slows.
const FOOT: f32 = 0.16;
const NEAR_GLASS: f32 = 0.12;
/// The seed of a call being placed: how far off the centre it is fed, how
/// wide, and how much dye and light it is fed.
const SEED: (f32, f32, f32, f32) = (0.08, 0.14, 1.3, 0.5);

/// What of a voice shows at once, before the fluid has moved: it shows the
/// fluid fuller, swells it towards the eye, by this much, and sends rings out
/// through it from the centre, a long wave for a low band and short ripples
/// for a high one (in waves per radius), this deep, travelling this many
/// radii a second, brighter by `CREST` at the crests.
const SWELL: f32 = 0.2;
const RIPPLE_RINGS: [f32; BANDS] = [1.0, 1.6, 2.6, 4.0, 6.0];
const RIPPLE_DEPTH: [f32; BANDS] = [0.07, 0.06, 0.045, 0.03, 0.02];
const RIPPLE_SPEED: f32 = 0.9;
const CREST: f32 = 1.6;
/// Seconds for the touch to rise to a sound and to fall after it: the
/// ripples, the swell, and how full the fluid shows.
const TOUCH: [(f32, f32); 3] = [(0.03, 0.16), (0.06, 0.22), (0.05, 0.35)];

/// Something pushing the fluid at one place, and feeding it there.
#[derive(Clone, Copy)]
struct Push {
    at: (f32, f32),
    force: (f32, f32),
    /// The dye it feeds, and the light: Codex's, and the user's.
    feed: f32,
    glow: f32,
    cool: f32,
    foot: f32,
}

/// The voices as they show on the frame they are heard.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Touch {
    /// Towards the eye, as a voice speaks.
    swell: f32,
    /// The rings of each band of the voices.
    ripple: Bands,
    /// How loud the call is, for how full the fluid shows.
    loud: f32,
}

impl Touch {
    /// Follows `voice` over `dt` seconds: fast to a sound, slower after it.
    fn follow(&mut self, voice: &Voice, dt: f32) {
        let ease = |value: &mut f32, to: f32, (rise, fall): (f32, f32)| {
            let pace = if to > *value { rise } else { fall };
            *value += (to - *value) * (1.0 - (-dt / pace).exp());
        };
        // The body of a voice is in its low bands.
        let body = |bands: &Bands| 0.5 * bands[0] + 0.3 * bands[1] + 0.2 * bands[2];
        let mut loud = 0.0f32;
        for band in 0..BANDS {
            let level = voice.agent[band].max(voice.user[band]);
            ease(&mut self.ripple[band], level, TOUCH[0]);
            loud = loud.max(level);
        }
        let body = body(&voice.agent).max(body(&voice.user));
        ease(&mut self.swell, SWELL * body, TOUCH[1]);
        ease(&mut self.loud, loud, TOUCH[2]);
    }
}

impl Fluid {
    fn new(n: usize) -> Self {
        let cells = n * n;
        let h = 2.0 / n as f32;
        let (mut inside, mut wall) = (vec![0.0; cells], vec![0.0; cells]);
        for j in 0..n {
            for i in 0..n {
                let (x, y) = ((i as f32 + 0.5) * h - 1.0, (j as f32 + 0.5) * h - 1.0);
                let r = x.hypot(y);
                inside[j * n + i] = if r < 1.0 { 1.0 } else { 0.0 };
                wall[j * n + i] = ((1.0 - r) / NEAR_GLASS).clamp(0.0, 1.0);
            }
        }
        let mut fluid = Self {
            n,
            u: vec![0.0; cells],
            v: vec![0.0; cells],
            dye: vec![0.0; cells],
            tone: vec![0.0; cells],
            heat: vec![0.0; cells],
            cool: vec![0.0; cells],
            a: vec![0.0; cells],
            b: vec![0.0; cells],
            c: vec![0.0; cells],
            d: vec![0.0; cells],
            e: vec![0.0; cells],
            inside,
            wall,
            time: 0.0,
            heard: [[0.0; BANDS]; 2],
        };
        // It starts as its own mist, so a still orb is not empty.
        for index in 0..cells {
            let (x, y) = fluid.at(index);
            fluid.dye[index] = fluid.mist(x, y) * fluid.inside[index];
            fluid.tone[index] = fluid.tones(x, y);
        }
        fluid
    }

    /// The centre of cell `index`.
    #[inline]
    fn at(&self, index: usize) -> (f32, f32) {
        let h = 2.0 / self.n as f32;
        (
            ((index % self.n) as f32 + 0.5) * h - 1.0,
            ((index / self.n) as f32 + 0.5) * h - 1.0,
        )
    }

    /// The mist the dye settles back towards: clouds and clear, drifting.
    fn mist(&self, x: f32, y: f32) -> f32 {
        let t = self.time;
        let table = lattice();
        let cloud = 0.65 * noise(table, x * 1.5 + 3.0 + t * 0.012, y * 1.5 + 7.0 - t * 0.009)
            + 0.35
                * noise(
                    table,
                    x * 3.1 + 21.0 - t * 0.016,
                    y * 3.1 + 13.0 + t * 0.011,
                );
        0.2 + 0.7 * smooth(0.3, 0.7, cloud)
    }

    fn tones(&self, x: f32, y: f32) -> f32 {
        let t = self.time;
        smooth(
            0.3,
            0.7,
            noise(
                lattice(),
                x * 1.1 + 11.0 - t * 0.008,
                y * 1.1 + 5.0 + t * 0.01,
            ),
        )
    }

    /// `field` at a point of the vessel's box, between its cells.
    #[inline]
    fn sample(field: &[f32], n: usize, x: f32, y: f32) -> f32 {
        let scale = n as f32 / 2.0;
        let gx = ((x + 1.0) * scale - 0.5).clamp(0.0, n as f32 - 1.001);
        let gy = ((y + 1.0) * scale - 0.5).clamp(0.0, n as f32 - 1.001);
        let (x0, y0) = (gx as usize, gy as usize);
        let (fx, fy) = (gx - x0 as f32, gy - y0 as f32);
        let row = y0 * n + x0;
        let top = field[row] + (field[row + 1] - field[row]) * fx;
        let bottom = field[row + n] + (field[row + n + 1] - field[row + n]) * fx;
        top + (bottom - top) * fy
    }

    /// The central difference of `field` at cell (`i`, `j`), along x and y.
    #[inline]
    fn gradient(field: &[f32], n: usize, i: usize, j: usize) -> (f32, f32) {
        let h = 2.0 / n as f32;
        let (left, right) = (i.saturating_sub(1), (i + 1).min(n - 1));
        let (up, down) = (j.saturating_sub(1), (j + 1).min(n - 1));
        (
            (field[j * n + right] - field[j * n + left]) / ((right - left) as f32 * h),
            (field[down * n + i] - field[up * n + i]) / ((down - up) as f32 * h),
        )
    }

    /// What pushes the fluid now: the two voices, band by band, and what
    /// `mood` adds.
    fn pushes(&mut self, dt: f32, voice: &Voice, mood: &Mood) -> Vec<Push> {
        use std::f32::consts::TAU;
        let t = self.time;
        let mut pushes = Vec::with_capacity(2 * BANDS + 3);
        let follow = 1.0 - (-dt / ONSET).exp();
        for (side, bands) in [voice.user, voice.agent].into_iter().enumerate() {
            for (band, level) in bands.into_iter().enumerate() {
                // A syllable is a kick: what the band has above its recent
                // average.
                let heard = &mut self.heard[side][band];
                let onset = (level - *heard).max(0.0);
                *heard += (level - *heard) * follow;
                let drive = VOICE.0 * level + VOICE.1 * onset;
                if drive < 0.004 {
                    continue;
                }
                // Each band has its own place round the centre, two fifths
                // of a turn from the last, so neighbours in pitch are not
                // neighbours in space, and the places come round slowly. It
                // goes straight out, the bands curling one way and the
                // other, so speaking bursts and does not turn.
                let user = side == 0;
                let place = band as f32 * 0.4 + if user { USER_PLACE } else { 0.0 };
                let (sin, cos) = (0.6 - t * 0.2 + place * TAU).sin_cos();
                let curl = if band % 2 == 0 { 1.0 } else { -1.0 } * BAND_CURL[band];
                let force = DRIVE * BAND_PUSH[band] * drive;
                let light = BAND_GLOW[band] * drive;
                pushes.push(Push {
                    at: (0.15 * cos, 0.15 * sin),
                    force: (force * (cos - curl * sin), force * (sin + curl * cos)),
                    feed: BAND_BODY[band] * drive,
                    glow: if user { 0.0 } else { light },
                    cool: if user { light } else { 0.0 },
                    foot: BAND_FOOT[band],
                });
            }
        }
        // The call being placed: a seed of light just off the centre, which
        // the turn winds into a small spiral.
        if mood.seed > 0.001 {
            let (sin, cos) = (t * 1.3).sin_cos();
            pushes.push(Push {
                at: (SEED.0 * cos, SEED.0 * sin),
                force: (0.0, 0.0),
                feed: SEED.2 * mood.seed,
                glow: SEED.3 * mood.seed,
                cool: 0.0,
                foot: SEED.1,
            });
        }
        // The call working: the turn itself is a force everywhere; this
        // only feeds it a thread of dye off-centre, which it winds into a
        // spiral.
        if mood.turn > 0.001 {
            let (sin, cos) = (-t * 0.35).sin_cos();
            pushes.push(Push {
                at: (0.4 * cos, 0.4 * sin),
                force: (0.0, 0.0),
                feed: 0.45 * mood.turn,
                glow: 0.45 * mood.turn,
                cool: 0.0,
                foot: FOOT,
            });
        }
        // Something being looked for: a stirrer circling near the glass,
        // pushing ahead of itself and leaving a lit trail.
        if mood.scan > 0.001 {
            let (sin, cos) = (t * 1.7).sin_cos();
            pushes.push(Push {
                at: (0.55 * cos, 0.55 * sin),
                force: (-SCAN * mood.scan * sin, SCAN * mood.scan * cos),
                feed: 0.6 * mood.scan,
                glow: 0.6 * mood.scan,
                cool: 0.0,
                foot: FOOT,
            });
        }
        // The call waiting on an answer: a puff from below, like a pulse.
        if mood.pulse > 0.001 {
            let beat = ((t / 1.6).fract() - 0.15) / 0.08;
            let beat = (-beat * beat).exp();
            pushes.push(Push {
                at: (0.0, 0.45),
                force: (0.0, -PULSE * mood.pulse * beat),
                feed: 0.8 * mood.pulse * beat,
                glow: 0.8 * mood.pulse * beat,
                cool: 0.0,
                foot: FOOT,
            });
        }
        pushes
    }

    /// Carries the fluid `dt` seconds on, pushed by `voice` and stirred as
    /// `mood` has it.
    fn step(&mut self, dt: f32, voice: &Voice, mood: &Mood) {
        let n = self.n;
        let cells = n * n;
        let table = lattice();
        self.time += dt;
        let t = self.time;
        let pushes = self.pushes(dt, voice, mood);
        // A slow stream function that changes with time keeps it alive: the
        // flow along its contours has no divergence.
        for index in 0..cells {
            let (x, y) = self.at(index);
            self.a[index] = noise(table, x * 1.3 + t * 0.05, y * 1.3 - t * 0.04)
                + 0.5 * noise(table, x * 2.7 - t * 0.07 + 9.0, y * 2.7 + t * 0.06 + 4.0);
        }
        let (stir, turn) = (STIR * mood.stir, TURN * mood.turn);
        for j in 0..n {
            for i in 0..n {
                let index = j * n + i;
                let (x, y) = self.at(index);
                let (dx, dy) = Self::gradient(&self.a, n, i, j);
                let free = (1.0 - x * x - y * y).max(0.0);
                let (mut fx, mut fy) = (stir * dy - turn * y * free, -stir * dx + turn * x * free);
                let (mut feed, mut glow, mut cool) = (0.0, 0.0, 0.0);
                for push in &pushes {
                    let (px, py) = (x - push.at.0, y - push.at.1);
                    let reach = (-(px * px + py * py) / (2.0 * push.foot * push.foot)).exp();
                    fx += push.force.0 * reach;
                    fy += push.force.1 * reach;
                    feed += push.feed * reach;
                    glow += push.glow * reach;
                    cool += push.cool * reach;
                }
                self.c[index] = feed;
                self.d[index] = glow;
                self.e[index] = cool;
                self.u[index] += dt * fx;
                self.v[index] += dt * fy;
            }
        }
        // The small eddies the grid smooths away are put back: a push round
        // each one, towards where it is strongest, harder while a voice's
        // high bands sound.
        let fine = |bands: &Bands| 0.5 * (bands[3] + bands[4]);
        let eddies = EDDIES.0 + EDDIES.1 * fine(&voice.user).max(fine(&voice.agent));
        for j in 0..n {
            for i in 0..n {
                let (vx, _) = Self::gradient(&self.v, n, i, j);
                let (_, uy) = Self::gradient(&self.u, n, i, j);
                self.a[j * n + i] = vx - uy;
                self.b[j * n + i] = (vx - uy).abs();
            }
        }
        for j in 0..n {
            for i in 0..n {
                let index = j * n + i;
                let (gx, gy) = Self::gradient(&self.b, n, i, j);
                let length = gx.hypot(gy) + 1e-5;
                self.u[index] += dt * eddies * (gy / length) * self.a[index];
                self.v[index] -= dt * eddies * (gx / length) * self.a[index];
            }
        }
        // The flow carries itself, thickens, and comes to rest at the glass.
        let drag = (-dt * mood.drag).exp();
        for index in 0..cells {
            let (x, y) = self.at(index);
            let (bx, by) = (x - dt * self.u[index], y - dt * self.v[index]);
            let keep = drag * self.wall[index];
            self.a[index] = Self::sample(&self.u, n, bx, by) * keep;
            self.b[index] = Self::sample(&self.v, n, bx, by) * keep;
        }
        std::mem::swap(&mut self.u, &mut self.a);
        std::mem::swap(&mut self.v, &mut self.b);
        // What flows into a cell flows out of it: the pressure that makes it
        // so, with the glass as a wall, taken out of the flow.
        let h = 2.0 / n as f32;
        for j in 0..n {
            for i in 0..n {
                let (ux, _) = Self::gradient(&self.u, n, i, j);
                let (_, vy) = Self::gradient(&self.v, n, i, j);
                self.a[j * n + i] = (ux + vy) * h * h;
            }
        }
        self.b.fill(0.0);
        for _ in 0..PRESSURE_ROUNDS {
            for j in 0..n {
                for i in 0..n {
                    let index = j * n + i;
                    if self.inside[index] == 0.0 {
                        continue;
                    }
                    let here = self.b[index];
                    // A neighbour in the glass pushes back as hard as here.
                    let near = |at: usize, open: bool| -> f32 {
                        if open && self.inside[at] > 0.0 {
                            self.b[at]
                        } else {
                            here
                        }
                    };
                    let sum = near(index.wrapping_sub(1), i > 0)
                        + near(index + 1, i + 1 < n)
                        + near(index.wrapping_sub(n), j > 0)
                        + near(index + n, j + 1 < n);
                    self.b[index] = (sum - self.a[index]) * 0.25;
                }
            }
        }
        for j in 0..n {
            for i in 0..n {
                let index = j * n + i;
                let (px, py) = Self::gradient(&self.b, n, i, j);
                self.u[index] = (self.u[index] - px) * self.wall[index];
                self.v[index] = (self.v[index] - py) * self.wall[index];
            }
        }
        // The dyes ride the flow. What pushes the fluid feeds it dye and
        // light where it pushes; the dye settles back towards its mist, so
        // the vessel neither empties nor fills; the light fades.
        let fade = (-dt * SETTLE.2).exp();
        for index in 0..cells {
            let (x, y) = self.at(index);
            let (bx, by) = (x - dt * self.u[index], y - dt * self.v[index]);
            let dye = Self::sample(&self.dye, n, bx, by) + dt * FEED.0 * self.c[index];
            let tone = Self::sample(&self.tone, n, bx, by);
            let heat = Self::sample(&self.heat, n, bx, by) + dt * FEED.1 * self.d[index];
            let cool = Self::sample(&self.cool, n, bx, by) + dt * FEED.1 * self.e[index];
            // While the call is being placed there is no mist to settle
            // to: the glass is empty but for the seed.
            let mist = self.mist(x, y) * (1.0 - mood.seed);
            self.a[index] =
                (dye + dt * SETTLE.0 * (mist - dye)).clamp(0.0, 1.6) * self.inside[index];
            self.b[index] = (tone + dt * SETTLE.1 * (self.tones(x, y) - tone)).clamp(0.0, 1.0);
            self.c[index] = (heat * fade).clamp(0.0, 1.5) * self.inside[index];
            self.e[index] = (cool * fade).clamp(0.0, 1.5) * self.inside[index];
        }
        std::mem::swap(&mut self.dye, &mut self.a);
        std::mem::swap(&mut self.tone, &mut self.b);
        std::mem::swap(&mut self.heat, &mut self.c);
        std::mem::swap(&mut self.cool, &mut self.e);
    }

    /// The fluid in its glass, `size` pixels square, as BGRA with straight
    /// alpha. `energy` is how full the fluid shows, 0–1. Its colour is kept
    /// whole and how much of it there is goes in the alpha: where the fluid
    /// is thin the glass is clear, and it is clear all round the fluid,
    /// whose ball stops short of the glass. The whole square is filled: the painter clips it to the
    /// disc at the display's own resolution, so the silhouette is never this
    /// image's coarse one. `touch` is what the voices do to it at once.
    fn paint(&self, size: u32, energy: f32, mood: &Mood, touch: &Touch) -> Vec<u8> {
        let n = size as f32;
        let [shade, deep, lit, peak] = mood.colours;
        let rippling = touch.ripple.iter().any(|ripple| *ripple > 0.004);
        let mut pixels = vec![0u8; (size * size * 4) as usize];
        for (index, pixel) in pixels.chunks_exact_mut(4).enumerate() {
            let (column, row) = (index as u32 % size, index as u32 / size);
            let x = (column as f32 + 0.5) / n * 2.0 - 1.0;
            let y = (row as f32 + 0.5) / n * 2.0 - 1.0;
            // How squarely the glass faces the eye here: 1 at the centre, 0
            // at the limb.
            let glass = (1.0 - x * x - y * y).max(0.0).sqrt();
            // The fluid is a ball inside the glass, short of it all round:
            // however full the fluid is, there is clear glass about it. It
            // is seen through the sphere as through a lens.
            let (x, y) = (x / CORE, y / CORE);
            let facing = (1.0 - x * x - y * y).max(0.0).sqrt();
            let lens = 0.722 / (0.72 + 0.28 * facing);
            // The touch moves what is seen, not the fluid: none of it at
            // the glass, and no ring at the very centre.
            let open = facing * facing;
            let wave = if rippling {
                let r = (1.0 - open).sqrt();
                let at = r - RIPPLE_SPEED * self.time;
                let rings: f32 = (0..BANDS)
                    .map(|band| {
                        touch.ripple[band]
                            * RIPPLE_DEPTH[band]
                            * (std::f32::consts::TAU * RIPPLE_RINGS[band] * at).sin()
                    })
                    .sum();
                rings * r * open * 2.6
            } else {
                0.0
            };
            let warp = lens * (1.0 - touch.swell * open + wave);
            let (sx, sy) = (x * warp, y * warp);
            let dye = Self::sample(&self.dye, self.n, sx, sy);
            let tone = Self::sample(&self.tone, self.n, sx, sy);
            // Dark at rest; Codex's voice lights it to the accent, and to
            // the accent's light tone at its peaks; the user's brings its
            // own light.
            let warm = Self::sample(&self.heat, self.n, sx, sy);
            let heat = smooth(0.05, 0.9, warm);
            let colour = blend(blend(shade, deep, tone), lit, heat * 0.9);
            let colour = blend(colour, peak, smooth(0.7, 1.4, warm) * 0.7);
            // Where there is little dye there is none to see: the fluid has
            // clear water in it, and the surface behind shows through.
            let body = smooth(CLEAR.0, CLEAR.1, dye) * energy * (1.0 + 0.6 * heat);
            // The user's light shows on its own, in its own colour, wherever
            // it is: in the fluid or in clear water.
            let own = USER_SHOWS * smooth(0.05, 0.9, Self::sample(&self.cool, self.n, sx, sy));
            let colour = blend(colour, mood.user, own / (body + own).max(1e-4));
            let colour = colour.map(|c| c * (1.0 - mood.limb * (1.0 - facing).powi(2)));
            let fill = ((body + own)
                * (1.0 - smooth(CORE_EDGE, 1.0, (1.0 - open).sqrt()))
                * (0.7 + 0.3 * facing)
                * (1.0 + CREST * wave)
                * mood.dense
                * 1.15)
                .clamp(0.0, 1.0);
            // The reference's Fresnel atmosphere: a haze of the fluid's
            // colour in the glass itself, gathered at the limb. The fluid
            // is in front of it.
            // On a dark surface it gathers opposite the light, where a
            // glass ball glows with what passes through it, and leaves the
            // lit side to the rim: all round, it would read as a second
            // edge.
            let towards = (x * RIM_TOWARDS.0 + y * RIM_TOWARDS.1)
                / (x.hypot(y) * RIM_TOWARDS.0.hypot(RIM_TOWARDS.1)).max(1e-4);
            let away = 1.0 - smooth(-0.45, 0.8, towards);
            let gathered = CRESCENT.0 + CRESCENT.1 * away * away;
            let haze = (1.0 - glass).powf(FRESNEL)
                * HAZE
                * mood.dense
                * (1.0 + mood.crescent * (gathered - 1.0));
            let alpha = fill + haze * (1.0 - fill);
            let colour = blend(lit, colour, fill / alpha.max(1e-4));
            let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0) as u8;
            pixel.copy_from_slice(&[
                byte(colour[2]),
                byte(colour[1]),
                byte(colour[0]),
                byte(alpha),
            ]);
        }
        pixels
    }

    /// How hard the fluid turns about its centre: its angular momentum.
    #[cfg(test)]
    fn spin(&self) -> f32 {
        (0..self.u.len())
            .map(|index| {
                let (x, y) = self.at(index);
                x * self.v[index] - y * self.u[index]
            })
            .sum::<f32>()
            / self.u.len() as f32
    }

    /// The fastest the fluid moves anywhere, in vessel radii per second.
    #[cfg(test)]
    fn speed(&self) -> f32 {
        self.u
            .iter()
            .zip(&self.v)
            .map(|(u, v)| u.hypot(*v))
            .fold(0.0, f32::max)
    }
}

/// The glass's rim, `size` device pixels square, as BGRA with straight
/// alpha. On a dark surface it is the reference's: a thin band of light on
/// the silhouette, strongest up and to the left. On a light one light cannot
/// add, so the glass is told by its dark (a shade inside its edge and a
/// hairline on it), with that same band as a white glint and the light's
/// bloom on the sphere's shoulder.
fn paint_rim(size: u32, dark: bool) -> Vec<u8> {
    let n = size as f32;
    let px = 2.0 / n;
    let whole = RIM_FINE.2 + (1.0 - RIM_FINE.2) * smooth(RIM_FINE.0, RIM_FINE.1, n);
    let radius = 1.0 - 1.5 * px * whole;
    let towards = {
        let length = RIM_TOWARDS.0.hypot(RIM_TOWARDS.1);
        (RIM_TOWARDS.0 / length, RIM_TOWARDS.1 / length)
    };
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    for (index, pixel) in pixels.chunks_exact_mut(4).enumerate() {
        let (column, row) = (index as u32 % size, index as u32 / size);
        let u = (column as f32 + 0.5) / n * 2.0 - 1.0;
        let v = (row as f32 + 0.5) / n * 2.0 - 1.0;
        let r = u.hypot(v);
        if r >= 1.0 {
            continue;
        }
        let reach = px * whole;
        let band =
            whole * (1.0 - smooth(reach * RIM_BAND.0, reach * RIM_BAND.1, (r - radius).abs()));
        let facing_light = smooth(-0.45, 0.8, (u * towards.0 + v * towards.1) / r.max(1e-4));
        // On a light surface, the light on the glass itself, over whatever
        // is in it: its bloom on the sphere's shoulder and a sheen inside
        // the edge, towards the light and faintly away from it.
        let from = (u - towards.0 * BLOOM.0).hypot(v - towards.1 * BLOOM.0);
        let bloom = BLOOM.2 * (1.0 - smooth(0.0, BLOOM.1, from));
        let within = 1.0 - smooth(0.0, SHEEN.1, (r - SHEEN.0).abs());
        let away = 1.0 - facing_light;
        // On a small orb it would thicken the rim: it fades with the band.
        let sheen = within
            * (SHEEN.2 * facing_light.powi(3) + SHEEN.3 * away.powi(3))
            * smooth(RIM_FINE.0, RIM_FINE.1, n);
        let (ink, alpha) = if dark {
            // The reference's glass: one edge of light and nothing inside
            // it. A sheen there reads as a second border.
            (1.0, band * (0.18 + 0.82 * facing_light) * RIM_STRENGTH.0)
        } else {
            // Glass on a light surface is told by its dark: a shade inside
            // the edge and a hairline on it, both deeper away from the
            // light. Over them, the band as a white glint, and the bloom of
            // the light on the sphere's shoulder.
            let line = 1.0 - smooth(0.0, 1.5 * reach, (r - (1.0 - 0.75 * reach)).abs());
            let shadow = (smooth(0.84, 1.0, r) * RIM_SHADE * (0.45 + 0.75 * away)
                + line * (LIGHT_EDGE.0 + LIGHT_EDGE.1 * away))
                .min(1.0);
            let glint = band * facing_light * RIM_STRENGTH.1;
            let white = glint + (bloom + sheen) * (1.0 - glint);
            let alpha = white + shadow * (1.0 - white);
            (
                (white + RIM_SHADE_INK * shadow * (1.0 - white)) / alpha.max(1e-4),
                alpha,
            )
        };
        if alpha <= 0.0 {
            continue;
        }
        let ink = (ink.clamp(0.0, 1.0) * 255.0) as u8;
        pixel.copy_from_slice(&[ink, ink, ink, (alpha.clamp(0.0, 1.0) * 255.0) as u8]);
    }
    pixels
}

/// One grain of sand at a pixel: 0–255, the same every time for the same
/// `seed`.
#[inline]
fn grain(x: u32, y: u32, seed: u32) -> u32 {
    let mut hash = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263) ^ seed;
    hash = (hash ^ (hash >> 13)).wrapping_mul(1_274_126_177);
    (hash ^ (hash >> 16)) & 0xff
}

/// The fluid's image (`size` pixels square, BGRA) with its grain: redrawn
/// finer for a disc `across` device pixels wide, each pixel a little more
/// or less of the fluid than its neighbours. The grain is fixed in the
/// glass, so it shows where the fluid passes; `shimmer` of it (0–1) changes
/// from one instant of `time` to the next. In whole numbers throughout: it
/// runs on every pixel of every frame.
fn sand(size: u32, pixels: &[u8], across: u32, time: f32, shimmer: f32) -> (u32, Vec<u8>) {
    let finer = (across / size).clamp(1, GRAIN_FINER);
    let out = size * finer;
    let instant = ((time * 14.0) as u32).wrapping_mul(0x85eb_ca6b);
    let shimmer = ((shimmer * GRAIN_SHIMMER).clamp(0.0, 1.0) * 256.0) as u32;
    let strength = (GRAIN * 256.0) as i32;
    // Where each fine pixel falls between the fluid's own: the lower of the
    // two, the upper, and how far towards the upper, in 256ths.
    let places: Vec<(usize, usize, u32)> = (0..out)
        .map(|fine| {
            let at = ((fine as f32 + 0.5) / finer as f32 - 0.5).clamp(0.0, (size - 1) as f32);
            let low = at as usize;
            (
                low,
                (low + 1).min(size as usize - 1),
                ((at - low as f32) * 256.0) as u32,
            )
        })
        .collect();
    let row_bytes = size as usize * 4;
    let mut sanded = vec![0u8; (out * out * 4) as usize];
    for (row, line) in sanded.chunks_exact_mut(out as usize * 4).enumerate() {
        let (y0, y1, fy) = places[row];
        let (upper, lower) = (
            &pixels[y0 * row_bytes..(y0 + 1) * row_bytes],
            &pixels[y1 * row_bytes..(y1 + 1) * row_bytes],
        );
        for (column, pixel) in line.chunks_exact_mut(4).enumerate() {
            let (x0, x1, fx) = places[column];
            let (a, b) = (x0 * 4, x1 * 4);
            let mut mixed = [0u32; 4];
            for (channel, mixed) in mixed.iter_mut().enumerate() {
                let top =
                    u32::from(upper[a + channel]) * (256 - fx) + u32::from(upper[b + channel]) * fx;
                let bottom =
                    u32::from(lower[a + channel]) * (256 - fx) + u32::from(lower[b + channel]) * fx;
                *mixed = (top * (256 - fy) + bottom * fy) >> 16;
            }
            let (column, row) = (column as u32, row as u32);
            let still = grain(column, row, 0x9e37_79b9);
            let grain = if shimmer == 0 {
                still
            } else {
                (still * (256 - shimmer) + grain(column, row, instant) * shimmer) >> 8
            };
            // Up to `GRAIN` more of the fluid here, or less.
            let more = 256 + strength * (2 * grain as i32 - 255) / 255;
            let alpha = (mixed[3] as i32 * more) >> 8;
            pixel[0] = mixed[0] as u8;
            pixel[1] = mixed[1] as u8;
            pixel[2] = mixed[2] as u8;
            pixel[3] = alpha.clamp(0, 255) as u8;
        }
    }
    (out, sanded)
}

fn image(size: u32, pixels: Vec<u8>) -> Arc<RenderImage> {
    let buffer = image::RgbaImage::from_raw(size, size, pixels).expect("size squared pixels");
    Arc::new(RenderImage::new([image::Frame::new(buffer)]))
}

/// The two voices an orb follows, each as how loud its bands are, 0–1,
/// lowest first ([`super::hearing`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Voice {
    pub user: Bands,
    pub agent: Bands,
}

/// What an orb paints this frame.
#[derive(Clone)]
pub(crate) struct Frame {
    fluid: Arc<RenderImage>,
    rim: Arc<RenderImage>,
}

/// The orb between frames: the state it shows and the one waiting to become
/// it, where the fluid and its character actually are, and the images it
/// has on the GPU.
#[derive(Default)]
pub(crate) struct Motion {
    state: Option<State>,
    wanted: Option<(State, Instant)>,
    mood: Option<Mood>,
    body: Option<Fluid>,
    touch: Touch,
    breath: f32,
    ticked: Option<Instant>,
    fluid: Option<Arc<RenderImage>>,
    rim: Option<(u32, bool, Arc<RenderImage>)>,
}

impl Motion {
    /// Follows the call to `state` once it has lasted ([`State::hold`]).
    fn settle(&mut self, state: State, now: Instant) -> State {
        let Some(current) = self.state else {
            self.state = Some(state);
            return state;
        };
        if state == current {
            self.wanted = None;
            return current;
        }
        let since = match self.wanted {
            Some((wanted, since)) if wanted == state => since,
            _ => now,
        };
        if now.saturating_duration_since(since) >= current.hold(state) {
            self.state = Some(state);
            self.wanted = None;
            state
        } else {
            self.wanted = Some((state, since));
            current
        }
    }

    /// Advances the fluid to `now` and returns the frame to paint: it
    /// follows `state`, glides its character towards that state's, and
    /// carries the fluid on, stirred as the state and the voice have it. `resolution`
    /// is the fluid's own, `across` the disc's diameter in device pixels,
    /// for the rim. Under `reduced` motion the fluid stands still in each
    /// state's colours and brightness.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn frame(
        &mut self,
        state: State,
        voice: Voice,
        theme: &Theme,
        resolution: u32,
        across: u32,
        now: Instant,
        reduced: bool,
        window: &mut Window,
    ) -> Frame {
        let to = Mood::of(self.settle(state, now), theme);
        let dt = self
            .ticked
            .replace(now)
            .map_or(0.0, |at| now.saturating_duration_since(at).as_secs_f32())
            .min(STEP)
            / crate::motion::speed_scale();
        let mood = match self.mood {
            Some(mood) if !reduced => mood.towards(to, 1.0 - (-dt / GLIDE).exp()),
            _ => to,
        };
        let voice = if reduced { Voice::default() } else { voice };
        let dark = theme.appearance.is_dark();
        // A still fluid is drawn once per state, not once per frame.
        let still = reduced && self.mood == Some(mood);
        self.mood = Some(mood);
        let cells = if resolution >= GRID.0 { GRID.1 } else { GRID.2 };
        // A call being placed starts with an empty glass.
        let born = self.body.is_none() && mood.seed > 0.5 && !reduced;
        let body = self.body.get_or_insert_with(|| Fluid::new(cells));
        if born {
            body.dye.fill(0.0);
        }
        if !reduced && dt > 0.0 {
            body.step(dt, &voice, &mood);
            self.touch.follow(&voice, dt);
            self.breath += dt;
        } else if reduced {
            self.touch = Touch::default();
        }
        let fluid = match self.fluid.clone().filter(|_| still) {
            Some(fluid) => fluid,
            None => {
                let breath = if reduced {
                    0.5
                } else {
                    0.5 - 0.5 * (self.breath / BREATH_PERIOD * std::f32::consts::TAU).cos()
                };
                let energy = mood.rest + mood.lift * self.touch.loud + mood.breath * breath;
                let (size, pixels) = sand(
                    resolution,
                    &body.paint(resolution, energy, &mood, &self.touch),
                    across,
                    body.time,
                    self.touch.loud,
                );
                let fluid = image(size, pixels);
                if let Some(old) = self.fluid.replace(fluid.clone()) {
                    let _ = window.drop_image(old);
                }
                fluid
            }
        };
        let rim = match &self.rim {
            Some((size, was_dark, rim)) if (*size, *was_dark) == (across, dark) => rim.clone(),
            _ => {
                let rim_image = image(across, paint_rim(across, dark));
                if let Some((_, _, old)) = self.rim.replace((across, dark, rim_image.clone())) {
                    let _ = window.drop_image(old);
                }
                rim_image
            }
        };
        Frame { fluid, rim }
    }

    /// Lets go of the images once the orb is out of view.
    pub(crate) fn release(&mut self, window: &mut Window) {
        // The rim is kept: it costs a whole frame to compute at the call
        // screen's size, and the next entrance should not start with that.
        // Only its copy on the GPU goes.
        let rim = self.rim.take();
        for old in self
            .fluid
            .take()
            .into_iter()
            .chain(rim.iter().map(|(_, _, rim)| rim.clone()))
        {
            let _ = window.drop_image(old);
        }
        *self = Self {
            rim,
            ..Self::default()
        };
    }
}

/// How an orb is sized: the disc's diameter and the square it stands in.
#[derive(Clone, Copy)]
pub(crate) struct Shape {
    pub diameter: f32,
    pub room: f32,
}

/// The orb, centred in a square of `shape.room`. `alive` is 0 while the call
/// connects (the disc held back) and 1 once it is live; `disc` may name and
/// decorate the disc.
pub(crate) fn orb(
    shape: Shape,
    frame: Frame,
    alive: f32,
    disc: impl FnOnce(gpui::Div) -> gpui::AnyElement,
) -> gpui::Div {
    // The glass holds still while the call is live: only what is in it
    // moves.
    let diameter = shape.diameter * (WAITING_SIZE + (1.0 - WAITING_SIZE) * alive);
    let inset = (shape.room - diameter) / 2.0;
    let glass = gpui::canvas(
        |_, _, _| {},
        move |bounds: Bounds<gpui::Pixels>, _, window, _| {
            // Clipped to the disc here, at the display's resolution: the
            // fluid's own pixels are several times larger.
            let round = Corners::all(bounds.size.width / 2.0);
            for layer in [&frame.fluid, &frame.rim] {
                let _ = window.paint_image(bounds, round, layer.clone(), 0, false);
            }
        },
    )
    .absolute()
    .inset_0();
    div()
        .relative()
        .flex_none()
        .size(px(shape.room))
        .child(disc(
            div()
                .absolute()
                .top(px(inset))
                .left(px(inset))
                .size(px(diameter))
                .opacity(WAITING_PRESENCE + (1.0 - WAITING_PRESENCE) * alive)
                .child(glass),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{AccentColor, Appearance};

    #[test]
    fn the_orb_is_glass_round_a_fluid_that_the_voice_and_the_state_stir() {
        let theme = Theme::default();
        let mood = |state| Mood::of(state, &theme);
        let size = 64;
        let at = |pixels: &[u8], x: u32, y: u32| {
            let i = ((y * size + x) * 4) as usize;
            [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
        };
        let amount =
            |pixels: &[u8]| -> u32 { pixels.chunks_exact(4).map(|p| u32::from(p[3])).sum() };
        let calm = Touch::default();
        // A voice with syllables in it: a vowel in its low bands, a
        // consonant's hiss as each one starts.
        let speech = |t: f32| -> Bands {
            let (sin, cos) = (t * 5.1).sin_cos();
            let (vowel, hiss) = (sin.max(0.0), if sin > 0.0 { cos.max(0.0) } else { 0.0 });
            [
                0.5 * vowel,
                0.9 * vowel,
                0.6 * vowel,
                0.8 * hiss,
                0.6 * hiss,
            ]
        };
        let (user, codex): (fn(Bands) -> Voice, fn(Bands) -> Voice) = (
            |bands| Voice {
                user: bands,
                ..Voice::default()
            },
            |bands| Voice {
                agent: bands,
                ..Voice::default()
            },
        );
        let run =
            |fluid: &mut Fluid, state: State, seconds: f32, voice: &dyn Fn(f32) -> Voice| -> f32 {
                let mood = mood(state);
                (0..(seconds * 30.0) as usize).fold(0.0, |peak: f32, _| {
                    fluid.step(1.0 / 30.0, &voice(fluid.time), &mood);
                    peak.max(fluid.speed())
                })
            };
        let silence = |_: f32| Voice::default();
        // How lit the fluid is within a third of the way out, and beyond
        // half of it, by Codex's light or by the user's.
        let lit = |fluid: &Fluid, light: &[f32]| -> (f32, f32) {
            let mean = |from: f32, to: f32| {
                let cells = (0..light.len()).filter(|index| {
                    let (x, y) = fluid.at(*index);
                    (from..to).contains(&x.hypot(y))
                });
                cells.clone().map(|index| light[index]).sum::<f32>() / cells.count() as f32
            };
            (mean(0.0, 0.33), mean(0.5, 1.0))
        };
        // Left alone it drifts, slowly.
        let mut fluid = Fluid::new(GRID.1);
        let still = fluid.paint(size, 0.5, &mood(State::Listening), &calm);
        run(&mut fluid, State::Listening, 5.0, &silence);
        let idle = fluid.speed();
        assert!(idle > 0.03 && idle < 0.4, "{idle}");
        assert_ne!(
            fluid.paint(size, 0.5, &mood(State::Listening), &calm),
            still
        );
        let speaking = mood(State::Speaking);
        let quiet = fluid.paint(size, speaking.rest, &speaking, &calm);
        // A voice is what moves it, whatever the call's state says: several
        // times faster with each word, Codex's out from the centre…
        let spoken = run(&mut fluid, State::Listening, 2.0, &|t| codex(speech(t)));
        assert!(spoken > idle * 3.0 && spoken < 3.0, "{spoken} after {idle}");
        let (centre, glass) = lit(&fluid, &fluid.heat);
        assert!(centre > glass * 2.0, "{centre} {glass}");
        assert!(fluid.spin().abs() < 0.05, "{}", fluid.spin());
        // …and the user's alike, but it brings its own light, not Codex's.
        let mut heard = Fluid::new(GRID.1);
        run(&mut heard, State::Listening, 5.0, &silence);
        let listened = run(&mut heard, State::Listening, 2.0, &|t| user(speech(t)));
        assert!(listened > idle * 3.0 && listened < 3.0, "{listened}");
        let (centre, glass) = lit(&heard, &heard.cool);
        assert!(centre > glass * 2.0, "{centre} {glass}");
        assert!(heard.spin().abs() < 0.05, "{}", heard.spin());
        assert!(heard.heat.iter().all(|heat| *heat == 0.0));
        assert!(fluid.cool.iter().all(|cool| *cool == 0.0));
        // Silence lets that light fade.
        run(&mut heard, State::Listening, 8.0, &silence);
        assert!(lit(&heard, &heard.cool).0 < centre * 0.05);
        // Each has its own colour: Codex's is the accent, the user's the
        // surface's ink, so one is never taken for the other.
        for appearance in [Appearance::Dark, Appearance::Light] {
            for accent in AccentColor::ALL {
                let mood = Mood::of(
                    State::Listening,
                    &Theme::for_preferences(appearance, accent),
                );
                let vivid = |[r, g, b]: [f32; 3]| r.max(g).max(b) - r.min(g).min(b);
                assert!(
                    vivid(mood.user) < vivid(mood.colours[2]) * 0.6,
                    "{accent:?}"
                );
            }
        }
        // What either voice says shapes the flow: the low of a voice is
        // broad and thickens the fluid, the hiss of a consonant spins small
        // eddies and lights it.
        let said = |band: usize, by: fn(Bands) -> Voice| -> (f32, f32, f32) {
            let mut fluid = Fluid::new(GRID.1);
            let mut bands = [0.0; BANDS];
            bands[band] = 0.8;
            run(&mut fluid, State::Listening, 1.5, &|_| by(bands));
            let n = fluid.n;
            let (mut eddies, mut speed) = (0.0, 0.0);
            for j in 0..n {
                for i in 0..n {
                    let (vx, _) = Fluid::gradient(&fluid.v, n, i, j);
                    let (_, uy) = Fluid::gradient(&fluid.u, n, i, j);
                    eddies += (vx - uy).abs();
                    speed += fluid.u[j * n + i].hypot(fluid.v[j * n + i]);
                }
            }
            (
                eddies / speed,
                // The dye it has beyond what was there.
                fluid.dye.iter().sum::<f32>() - Fluid::new(GRID.1).dye.iter().sum::<f32>(),
                fluid
                    .heat
                    .iter()
                    .chain(&fluid.cool)
                    .cloned()
                    .fold(0.0, f32::max),
            )
        };
        for by in [codex, user] {
            let ((low_fine, low_body, low_light), (high_fine, high_body, high_light)) =
                (said(0, by), said(4, by));
            assert!(high_fine > low_fine * 1.4, "{high_fine} {low_fine}");
            assert!(low_body > high_body * 1.5, "{low_body} {high_body}");
            assert!(high_light > low_light * 1.5, "{high_light} {low_light}");
        }
        // It also shows on the frame it is heard, before the fluid has
        // moved: fuller, swelling towards the eye and rippled by each band.
        let mut touch = Touch::default();
        touch.follow(&codex(speech(0.2)), 1.0 / 30.0);
        assert!(touch.loud > 0.3 && touch.swell > 0.03, "{touch:?}");
        assert!(touch.ripple[1] > touch.ripple[4], "{touch:?}");
        let loud = fluid.paint(
            size,
            speaking.rest + speaking.lift * touch.loud,
            &speaking,
            &touch,
        );
        assert!(amount(&loud) > amount(&quiet) * 9 / 8);
        assert_ne!(
            fluid.paint(size, speaking.rest, &speaking, &touch),
            fluid.paint(size, speaking.rest, &speaking, &calm)
        );
        let mut drawn = Touch::default();
        drawn.follow(&user(speech(0.2)), 1.0 / 30.0);
        assert_eq!(drawn, touch);
        for _ in 0..60 {
            touch.follow(&Voice::default(), 1.0 / 30.0);
        }
        assert!(touch.loud < 0.02 && touch.swell.abs() < 0.01, "{touch:?}");
        // …and it is the accent the rest of the app shows, whichever is
        // chosen: the theme's own tones, never a hue of the orb's making.
        for appearance in [Appearance::Dark, Appearance::Light] {
            for accent in AccentColor::ALL {
                let theme = Theme::for_preferences(appearance, accent);
                let apart = |a: f32, b: f32| (a - b).abs().min(1.0 - (a - b).abs());
                let family = apart(theme.accent.h, theme.accent_strong.h) + 0.01;
                let pixels = fluid.paint(size, 1.0, &Mood::of(State::Speaking, &theme), &calm);
                for p in pixels.chunks_exact(4) {
                    let [b, g, r] = [p[0], p[1], p[2]].map(|c| f32::from(c) / 255.0);
                    let hue = Hsla::from(gpui::Rgba { r, g, b, a: 1.0 }).h;
                    assert!(
                        apart(hue, theme.accent.h) <= family,
                        "{accent:?} {appearance:?}: {hue} for {}",
                        theme.accent.h
                    );
                }
            }
        }
        // It has inertia: still moving a second after the last word, settled
        // long after.
        run(&mut fluid, State::Listening, 1.0, &silence);
        assert!(fluid.speed() > idle);
        run(&mut fluid, State::Listening, 12.0, &silence);
        assert!(fluid.speed() < spoken / 2.0);
        assert!(fluid.spin().abs() < 0.02, "{}", fluid.spin());
        // A state is a way of moving. Working winds it about its centre;
        // muting brings it to rest.
        run(&mut fluid, State::Thinking, 4.0, &silence);
        assert!(fluid.spin() > 0.08, "{}", fluid.spin());
        run(&mut fluid, State::Muted, 3.0, &silence);
        assert!(fluid.speed() < 0.05, "{}", fluid.speed());
        // It stays a fluid in a vessel: at rest at the glass, never blown up.
        for index in 0..fluid.u.len() {
            assert!(fluid.dye[index].is_finite() && fluid.dye[index] <= 1.6);
            if fluid.wall[index] == 0.0 {
                assert_eq!((fluid.u[index], fluid.v[index]), (0.0, 0.0));
            }
        }
        // A call being placed is the orb being born: an empty glass with a
        // seed of light at its centre, which fills the glass once the call
        // connects.
        let mut born = Fluid::new(GRID.1);
        born.dye.fill(0.0);
        run(&mut born, State::Loading, 3.0, &silence);
        let (seed, glass) = lit(&born, &born.dye);
        assert!(seed > 0.4 && glass < 0.02, "{seed} {glass}");
        run(&mut born, State::Listening, 3.0, &silence);
        assert!(lit(&born, &born.dye).1 > 0.25);
        // The fluid is a little sandy: drawn finer than it is computed, each
        // pixel a little more or less of it than its neighbours, and no
        // more or less of it in all.
        let mut thick = Fluid::new(GRID.1);
        thick.dye.fill(1.0);
        let smooth_fluid = thick.paint(size, 0.6, &speaking, &calm);
        let (finer, sanded) = sand(size, &smooth_fluid, size * 6, 0.0, 0.0);
        assert_eq!(finer, size * GRAIN_FINER);
        let middle = |pixels: &[u8], across: u32| -> Vec<f32> {
            (across * 3 / 8..across * 5 / 8)
                .flat_map(|y| (across * 3 / 8..across * 5 / 8).map(move |x| (x, y)))
                .map(|(x, y)| f32::from(pixels[((y * across + x) * 4 + 3) as usize]))
                .collect()
        };
        let rough = |alphas: &[f32]| -> f32 {
            alphas
                .windows(2)
                .map(|pair| (pair[0] - pair[1]).abs())
                .sum::<f32>()
                / alphas.len() as f32
        };
        let mean = |alphas: &[f32]| alphas.iter().sum::<f32>() / alphas.len() as f32;
        let (before, after) = (middle(&smooth_fluid, size), middle(&sanded, finer));
        assert!(rough(&after) > rough(&before) * 3.0 + 4.0);
        assert!((mean(&after) - mean(&before)).abs() < mean(&before) * 0.08);
        assert_eq!(
            sand(44, &fluid.paint(44, 0.9, &speaking, &calm), 44, 0.0, 0.0).0,
            44
        );
        // It is in glass, and the glass is there to see whatever is in it.
        // At rest the fluid has clear water in it…
        let rest = Fluid::new(GRID.1).paint(size, 0.5, &mood(State::Listening), &calm);
        let core = |pixels: &[u8], clear: bool| {
            (size / 4..size * 3 / 4)
                .flat_map(|y| (size / 4..size * 3 / 4).map(move |x| (x, y)))
                .filter(|(x, y)| (at(pixels, *x, *y)[3] < 8) == clear)
                .count()
        };
        assert!(core(&rest, true) > 100 && core(&rest, false) > 100);
        // …and however full it gets, it stops short of the glass: between
        // the two there is only the glass's own faint haze.
        let mut full = Fluid::new(GRID.1);
        full.dye.fill(1.6);
        let full = full.paint(size, 1.0, &speaking, &calm);
        assert!(at(&full, size / 2, size / 2)[3] > 200);
        let shell = at(&full, size / 2, size * 3 / 100)[3];
        assert!(shell > 0 && shell < 64, "{shell}");
        // The rim is a thin band on the silhouette, brighter up and to the
        // left, as on the reference; the middle of the glass is clear.
        let glass = paint_rim(size, true);
        assert_eq!(at(&glass, size / 2, size / 2)[3], 0);
        let band = |x: u32, y: u32| (0..4).map(|d| at(&glass, x + d, y + d)[3]).max().unwrap();
        let (near, far) = (size * 15 / 100, size * 85 / 100 - 4);
        assert!(
            band(near, near) > band(far, far) * 2,
            "{} {}",
            band(near, near),
            band(far, far)
        );
        // On the bar's small orb it is a hairline, not a collar.
        let lit = |size: u32| paint_rim(size, true).chunks_exact(4).map(|p| p[3]).max();
        let (row, stage) = (lit(44).unwrap(), lit(440).unwrap());
        assert!(u32::from(row) * 10 < u32::from(stage) * 6, "{row} {stage}");
    }

    #[test]
    fn the_orb_stays_with_a_speaker_through_the_pauses_and_glides_between_states() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        let mut orb = Motion::default();
        // A call's own phases are followed at once: connecting, then live.
        assert_eq!(orb.settle(State::Loading, at(0)), State::Loading);
        assert_eq!(orb.settle(State::Listening, at(16)), State::Listening);
        // A speaker starting is followed as soon as it is more than a blip.
        assert_eq!(orb.settle(State::Speaking, at(100)), State::Listening);
        assert_eq!(orb.settle(State::Speaking, at(260)), State::Speaking);
        // A pause in the sentence does not hand the orb back…
        orb.settle(State::Listening, at(600));
        assert_eq!(orb.settle(State::Listening, at(1_200)), State::Speaking);
        orb.settle(State::Speaking, at(1_300));
        orb.settle(State::Listening, at(1_400));
        assert_eq!(orb.settle(State::Listening, at(2_200)), State::Speaking);
        // …silence that lasts does, and what the user does is immediate.
        assert_eq!(orb.settle(State::Listening, at(2_400)), State::Listening);
        assert_eq!(orb.settle(State::Muted, at(2_416)), State::Muted);
        // Between two states the fluid is neither: it is on its way, in
        // character and in colour.
        let theme = Theme::default();
        let (listening, speaking) = (
            Mood::of(State::Listening, &theme),
            Mood::of(State::Speaking, &theme),
        );
        let between = listening.towards(speaking, 0.5);
        assert!(between != listening && between != speaking);
        assert!(between.rest > listening.rest && between.rest < speaking.rest);
        assert_eq!(listening.towards(speaking, 1.0), speaking);
    }
}
