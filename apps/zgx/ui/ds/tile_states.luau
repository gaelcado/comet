-- Every agent state as a shape of the mark's tiles (a study for Gaël, shown
-- on the gallery's Statuses page; not yet what StatusIcon draws). Each
-- state is its own silhouette, read without colour, its hue the second cue;
-- all are drawn with the mark's material (superellipse tiles, gap = a
-- tile ÷ φ⁴) on its grid: 5 × 3 at 14 and up (the mark's own, so a change
-- of state flips tiles in place, as a flip-dot board does), 3 × 3 at 10–12
-- and for sub-agents, 2 × 2 under 10 points.
--
-- `#` a lit tile, `o` a tile at rest, `.` none.
--
--   TileGlyph { state = "done", size = 14 }
--   TileMorph { from = "working", to = "done", size = 16 }  -- the flip, its
--   progress the shader's time (0 to 1)

local sweep = require("@self/sweep")
local motion = require("@self/motion")

local M = {}

export type State = "working" | "connecting" | "waiting" | "done" | "input" | "approval" | "plan" | "failed" | "limited" | "woke"

-- Each state's shape on each grid. Where a shape can't hold at 2 × 2, the
-- note says what stands in.
M.SHAPES = {
	-- The mark, the sweep's band through its gap.
	working = {
		[5] = { "oo#.#", "o#.##", "#.##o" },
		[3] = { "o#.", "#.#", ".#o" },
		[2] = { "##", "#." },
	},
	-- An ellipsis: the line still forming. (2 × 2: the mark, in blue.)
	connecting = {
		[5] = { ".....", "#.#.#", "....." },
		[3] = { "...", "#.#", "..." },
		[2] = { "##", "#." },
	},
	-- Paused: two columns.
	waiting = {
		[5] = { ".#.#.", ".#.#.", ".#.#." },
		[3] = { "#.#", "#.#", "#.#" },
		[2] = { "#.", "#." },
	},
	-- A speech bubble, its tail under it: it's your turn to speak.
	input = {
		[5] = { "#####", "#####", ".#..." },
		[3] = { "###", "###", ".#." },
		[2] = { "##", "##" },
	},
	-- A door, the gap its doorway: it waits for you to let it through.
	approval = {
		[5] = { "#####", "#...#", "#...#" },
		[3] = { "###", "#.#", "#.#" },
		[2] = { "##", "oo" },
	},
	-- A checklist: three lines, each with its lead tile. (2 × 2 can't hold
	-- a list apart from a gauge: a dot stands in.)
	plan = {
		[5] = { "#.###", "#.###", "#.###" },
		[3] = { "###", "...", "###" },
	},
	-- A check, in tiles.
	done = {
		[5] = { "....#", "#..#.", ".##.." },
		[3] = { "..#", "#.#", ".#." },
		[2] = { ".#", "#." },
	},
	-- A cross, in tiles.
	failed = {
		[5] = { ".#.#.", "..#..", ".#.#." },
		[3] = { "#.#", ".#.", "#.#" },
		[2] = { "#.", ".#" },
	},
	-- A gauge cut part-way: its capacity spent.
	limited = {
		[5] = { "##ooo", "##ooo", "##ooo" },
		[3] = { "#oo", "#oo", "#oo" },
		[2] = { "#o", "#o" },
	},
	-- Rising: a caret pointing up, from the floor.
	woke = {
		[5] = { "..#..", ".#.#.", "#...#" },
		[3] = { ".#.", "#.#", "..." },
		[2] = { "..", "#." },
	},
}

-- The grid a glyph takes at `size` points (`who` "subagent": 3 × 3).
function M.grid(size: number, who: ("agent" | "subagent")?): number
	if size < 10 then
		return 2
	elseif size <= 12 or who == "subagent" then
		return 3
	end
	return 5
end

export type Faces = { lit: number, rest: number, cols: number, rows: number }

-- A shape's faces: the bits (row by row) of its lit tiles and of those at
-- rest.
function M.faces(state: State, grid: number): Faces
	local rows = M.SHAPES[state][grid]
	local cols = #rows[1]
	local lit, rest, i = 0, 0, 0
	for _, row in rows do
		for c = 1, cols do
			local ch = row:sub(c, c)
			if ch == "#" then
				lit += 2 ^ i
			elseif ch == "o" then
				rest += 2 ^ i
			end
			i += 1
		end
	end
	return { lit = lit, rest = rest, cols = cols, rows = #rows }
end

-- The states in the order the Statuses page shows them, and the hue each
-- takes (waiting wears working's, as ds/status has it).
M.ORDER = {
	{ state = "working", hue = "working" },
	{ state = "connecting", hue = "connecting" },
	{ state = "waiting", hue = "working" },
	{ state = "input", hue = "input" },
	{ state = "approval", hue = "approval" },
	{ state = "plan", hue = "plan" },
	{ state = "done", hue = "done" },
	{ state = "failed", hue = "failed" },
	{ state = "limited", hue = "limited" },
	{ state = "woke", hue = "woke" },
} :: { { state: State, hue: string } }

-- One shader for both: a tile shows face A, then, as the flip reaches it
-- (the shader's time, 0 to 1, crossing the grid diagonal by diagonal from
-- the top left), turns edge-on and shows face B. A still glyph is A alone.
-- params[0]: the ink (a token); params[1]: a resting tile's share of its
-- alpha, the grid's columns and rows; params[2]: A's lit and resting bits,
-- B's lit and resting bits.
local SOURCE = string.format(
	[[
fn fill(input:FillInput)->vec4<f32> {
 let cols=input.params[1].y; let rows=input.params[1].z;
 let g=%.9f;
 let k=min(input.size.x/(cols+(cols-1.)*g),input.size.y/(rows+(rows-1.)*g));
 let o=(input.size-vec2(cols+(cols-1.)*g,rows+(rows-1.)*g)*k)*.5;
 let q=(input.p-o)/(k*(1.+g));
 let cell=floor(q);
 if cell.x<0.||cell.y<0.||cell.x>=cols||cell.y>=rows { return vec4(0.); }
 let i=u32(cell.y*cols+cell.x);
 // Where its flip is: each tile turns over in 40%% of the time, the last
 // diagonal starting at 60%%.
 let d=(cell.x+cell.y)/max(1.,cols+rows-2.);
 let turn=clamp((input.time-.6*d)/.4,0.,1.);
 let angle=turn*3.14159265;
 let cs=cos(angle);
 let b=turn>.5;
 let lit=((u32(select(input.params[2].x,input.params[2].z,b))>>i)&1u)==1u;
 let rest=((u32(select(input.params[2].y,input.params[2].w,b))>>i)&1u)==1u;
 if !(lit||rest) { return vec4(0.); }
 // A squircle, |x|⁴ + |y|⁴ = 1, narrowed as it turns edge-on.
 let f=abs(fract(q)*(1.+g)-.5)*2./vec2(max(.04,abs(cs)),1.);
 let f2=f*f;
 let n=sqrt(sqrt(f2.x*f2.x+f2.y*f2.y));
 let cover=clamp(.5-(n-1.)*.5*k*abs(cs)*input.scale,0.,1.);
 let ink=input.params[0];
 let a=select(input.params[1].x,1.,lit)*(1.-.4*(1.-abs(cs)));
 return vec4(ink.rgb,ink.a*a*cover);
}
]],
	1 / ((1 + math.sqrt(5)) / 2) ^ 4
)
local shader: any = nil

local function node(size: number, a: Faces, b: Faces, ink: string): ContainerProps
	shader = shader or ui.shader(SOURCE)
	return {
		w = size,
		h = size,
		shrink = 0,
		bg_shader = shader,
		shader_params = {
			ink,
			{ motion.presence.rest, a.cols, a.rows, 0 },
			{ a.lit, a.rest, b.lit, b.rest },
		},
	}
end

-- A state's shape, `size` points, in its hue (or `ink`).
function M.TileGlyph(props: { state: State, size: number?, who: ("agent" | "subagent")?, ink: string? }): Node
	local size = props.size or 14
	local grid = M.grid(size, props.who)
	local ink = props.ink or `$zeron.status.{props.state}`
	if not M.SHAPES[props.state][grid] then
		-- (No shape this small: the dot.)
		return ui.row {
			w = size,
			h = size,
			shrink = 0,
			items = "center",
			justify = "center",
			ui.div { w = size * 0.7, h = size * 0.7, rounded = "$radius.full", bg = ink },
		}
	end
	local faces = M.faces(props.state, grid)
	return ui.div(node(size, faces, faces, ink))
end

-- One state's shape flipping into another's, over and over (a study):
-- `from` holds, its tiles turn over into `to`, which holds, and back.
function M.TileMorph(props: { from: State, to: State, size: number?, ink: string?, hold: number?, flip: number? }): Node
	local size = props.size or 16
	local grid = M.grid(size)
	local props_ = node(size, M.faces(props.from, grid), M.faces(props.to, grid), props.ink or `$zeron.status.{props.to}`)
	local hold, flip = props.hold or 900, props.flip or 600
	props_.on_visible = function(visible: boolean, el: Element)
		if visible and not motion.reduced_now() then
			anime.animate(el, {
				shaderTime = { 0, 1 },
				duration = flip,
				delay = hold,
				loop = true,
				alternate = true,
				loopDelay = hold,
				ease = "inOut(2)",
			})
		else
			anime.utils.remove(el)
		end
	end
	return ui.div(props_)
end

return M
