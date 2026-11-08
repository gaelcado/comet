-- Every agent state as the mark itself (a study for Gaël, shown on the
-- gallery's Statuses page; not yet what StatusIcon draws). Each state is one
-- idea on the mark's tiles, read by shape first and its hue second:
--
-- - working: the sweep's band through the gap; connecting the same, in blue
-- - waiting: every tile at rest
-- - done: every tile lit
-- - input: at rest, one tile lit (the agent turning to you)
-- - approval: at rest, the gap filled (a door held open for you)
-- - plan ready: the tiles drawn, not filled (a blueprint)
-- - failed: lit, one tile dropped (the mark broken)
-- - limited: lit up to the gap, at rest past it (stopped half way)
-- - woke: at rest, the sweep's first tiles lit (waking)
--
-- `TileGlyph { state = "input", size = 14, who = "agent" }`, `variant =
-- "b"` for a state's alternative. Its hue: the state's (ds/tokens).

local sweep = require("@self/sweep")
local motion = require("@self/motion")

local M = {}

export type State = "working" | "connecting" | "waiting" | "done" | "input" | "approval" | "plan" | "failed" | "limited" | "woke"

type Cell = { r: number, c: number, bit: number, d: number }
type Grid = { cols: number, rows: number, cells: { Cell }, gap: { Cell }, last: number }

-- A glyph's tiles and its gap (the cells the shape leaves out).
local function grid(glyph: sweep.Glyph): Grid
	local rows = sweep.GLYPHS[glyph].rows
	local cols = #rows[1]
	local out: Grid = { cols = cols, rows = #rows, cells = {}, gap = {}, last = cols - 1 + #rows - 1 }
	for r, row in rows do
		for c = 1, cols do
			local cell = { r = r - 1, c = c - 1, bit = (r - 1) * cols + (c - 1), d = (r - 1) + (c - 1) }
			table.insert(if row:sub(c, c) == "#" then out.cells else out.gap, cell)
		end
	end
	return out
end

local function mask(cells: { Cell }, keep: (cell: Cell) -> boolean): number
	local n = 0
	for _, cell in cells do
		if keep(cell) then
			n += 2 ^ cell.bit
		end
	end
	return n
end
local function all(): boolean
	return true
end

-- The tile nearest the grid's middle (ties: the first): the one a failure
-- drops.
local function middle(g: Grid): Cell
	local best, far = g.cells[1], math.huge
	for _, cell in g.cells do
		local dist = math.abs(cell.r - (g.rows - 1) / 2) + math.abs(cell.c - (g.cols - 1) / 2)
		if dist < far then
			best, far = cell, dist
		end
	end
	return best
end
-- The rightmost tile of a row (0: the top).
local function last_of(g: Grid, row: number): Cell
	local best = g.cells[1]
	for _, cell in g.cells do
		if cell.r == row and (best.r ~= row or cell.c > best.c) then
			best = cell
		end
	end
	return best
end

export type Faces = { lit: number, rest: number, outline: number }

-- A state's faces on `glyph`: which tiles are lit, at rest, drawn as an
-- outline (bits row by row; a bit in none is not drawn).
function M.faces(state: State, glyph: sweep.Glyph, variant: string?): Faces
	local g = grid(glyph)
	local b = variant == "b"
	local function split(lit: (cell: Cell) -> boolean): Faces
		return {
			lit = mask(g.cells, lit),
			rest = mask(g.cells, function(cell)
				return not lit(cell)
			end),
			outline = 0,
		}
	end
	if state == "working" or state == "connecting" then
		local band = sweep.GLYPHS[glyph].still
		return split(function(cell)
			return math.abs(cell.d - band) <= 1
		end)
	elseif state == "waiting" then
		return split(function()
			return false
		end)
	elseif state == "done" then
		return split(all)
	elseif state == "input" then
		local one = last_of(g, if b then g.rows - 1 else 0)
		return split(function(cell)
			return cell == one
		end)
	elseif state == "approval" then
		local faces = split(function()
			return false
		end)
		if b then
			faces.outline = mask(g.gap, all)
		else
			faces.lit = mask(g.gap, all)
		end
		return faces
	elseif state == "plan" then
		if b then
			return split(function(cell)
				return cell.r == 0
			end)
		end
		return { lit = 0, rest = 0, outline = mask(g.cells, all) }
	elseif state == "failed" then
		local dropped = middle(g)
		return {
			lit = mask(g.cells, function(cell)
				return cell ~= dropped
			end),
			rest = 0,
			outline = 0,
		}
	elseif state == "limited" then
		return split(function(cell)
			return cell.d < g.last / 2
		end)
	else -- woke
		return split(function(cell)
			return cell.d <= (if b then 0 else 1)
		end)
	end
end

-- The states in the order the Statuses page shows them, the hue each takes
-- and the ones with an alternative ("b").
M.ORDER = {
	{ state = "working", hue = "working" },
	{ state = "connecting", hue = "connecting" },
	{ state = "waiting", hue = "working" },
	{ state = "input", hue = "input", b = true },
	{ state = "approval", hue = "approval", b = true },
	{ state = "plan", hue = "plan", b = true },
	{ state = "done", hue = "done" },
	{ state = "failed", hue = "failed" },
	{ state = "limited", hue = "limited" },
	{ state = "woke", hue = "woke", b = true },
} :: { { state: State, hue: string, b: boolean? } }

-- params[0]: the ink (a token); params[1]: the rest's share of its alpha,
-- the glyph's id; params[2]: the lit, rest and outlined tiles' bits.
local SOURCE = string.format(
	[[
fn fill(input:FillInput)->vec4<f32> {
 let id=u32(input.params[1].y+.5);
 var cols=5.; var rows=3.;
 if id==1u { cols=3.; } else if id==2u { cols=4.; rows=2.; } else if id==3u { cols=2.; rows=2.; }
 let g=%.9f;
 let k=min(input.size.x/(cols+(cols-1.)*g),input.size.y/(rows+(rows-1.)*g));
 let o=(input.size-vec2(cols+(cols-1.)*g,rows+(rows-1.)*g)*k)*.5;
 let q=(input.p-o)/(k*(1.+g));
 let cell=floor(q);
 if cell.x<0.||cell.y<0.||cell.x>=cols||cell.y>=rows { return vec4(0.); }
 let i=u32(cell.y*cols+cell.x);
 let lit=((u32(input.params[2].x)>>i)&1u)==1u;
 let rest=((u32(input.params[2].y)>>i)&1u)==1u;
 let line=((u32(input.params[2].z)>>i)&1u)==1u;
 if !(lit||rest||line) { return vec4(0.); }
 // A squircle, |x|⁴ + |y|⁴ = 1; an outline its rim, a pixel or a sixth of
 // a tile thick.
 let f=abs(fract(q)*(1.+g)-.5)*2.;
 let f2=f*f;
 let n=sqrt(sqrt(f2.x*f2.x+f2.y*f2.y));
 var cover=clamp(.5-(n-1.)*.5*k*input.scale,0.,1.);
 var a=1.;
 if !lit && line {
  let t=1.-2.*max(1./input.scale,k*.16)/k;
  cover=cover-clamp(.5-(n/t-1.)*.5*k*t*input.scale,0.,1.);
 } else if !lit { a=input.params[1].x; }
 let ink=input.params[0];
 return vec4(ink.rgb,ink.a*a*max(cover,0.));
}
]],
	1 / ((1 + math.sqrt(5)) / 2) ^ 4
)
local shader: any = nil

-- A state's mark, `size` points: the mark at 14 and up, its 4 × 2 cut at 12,
-- the 2 × 2 micro form under 10, or a sub-agent's dot (`who`).
function M.TileGlyph(props: { state: State, size: number?, who: ("agent" | "subagent")?, variant: string?, ink: string? }): Node
	local size = props.size or 14
	local glyph = sweep.glyph(props.who, size)
	local faces = M.faces(props.state, glyph, props.variant)
	shader = shader or ui.shader(SOURCE)
	return ui.div {
		w = size,
		h = size,
		shrink = 0,
		bg_shader = shader,
		shader_params = {
			props.ink or `$zeron.status.{props.state}`,
			{ motion.presence.rest, sweep.GLYPHS[glyph].id, 0, 0 },
			{ faces.lit, faces.rest, faces.outline, 0 },
		},
	}
end

return M
