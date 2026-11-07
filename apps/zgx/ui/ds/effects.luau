-- A new chat's wallpaper effects: each a layer shader over the whole
-- picture as it's composited, so the art in it (a picture, or what a
-- plugin draws) takes one pass, not one a layer.
--
-- `ui.div { layer_shader = effects.shader("dither"), layer_params = effects.params { … }, … }`
--
-- One tone for every effect. Each works in linear light on the picture's
-- own grid (the picture fitted to 2048 a side, as Zeron's proxy of it, so
-- the cells hold to the picture and grow and shrink with it), and prints
-- each cell with ink on paper so that, seen from a step back, the cell is
-- the picture's own colour there:
--
-- 1. the picture recedes into the paper by one small offset (`RECEDE`);
-- 2. a cell's ink covers a share of it from `FLOOR` to `MOST`, by how far
--    its colour stands from the paper (`fx_share`);
-- 3. the ink lies on the line from the paper through that colour, further
--    out (no more than `PUSH`), and the gaps between make up the rest, so
--    the cell's mean is the colour's exactly (`fx_split`).
--
-- Switching effects changes the texture, never how bright or how coloured
-- the picture is. The paper follows the scheme (near black in the dark,
-- warm white in the light), the same rule for every effect. Every edge is
-- worked out over the window's own pixel (crisp at 2x, no shimmer), and
-- every effect, None too, fades the art from its top, full, to nothing at
-- its foot.
--
-- Each effect has a few candidates (`M.VARIANTS`); `M.VARIANT` is the one
-- the app draws, `"dither/silk"` names another.


local M = {}

-- The longer side of the picture's grid (Zeron's proxy of the picture).
M.GRID_MAX = 2048

-- The paper, by scheme (sRGB): near black, warm white. Fixed, not the
-- card's fill: a theme's grey card under the ink flattened the dark (the
-- texture a fifth to a half fainter), and every theme draws the same.
M.PAPER = { dark = { 0.043, 0.039, 0.055 }, light = { 0.969, 0.961, 0.945 } }
-- How much of a cell the ink covers, from the paper's own colour to the
-- edge of the colours (the same for every effect): never less than
-- `FLOOR` (the dark of a picture keeps a quiet texture, not a few bright
-- specks), never more than `MOST` (the ink stays a mark on the paper, the
-- paper taking the rest of the colour where it's bright).
M.FLOOR, M.MOST = 0.12, 0.62
-- By scheme: how far past the colour the ink may go from the paper (2.2:
-- more than twice as far), so it reads at a step back yet stays the
-- picture's colour, deeper, not a printer's primary; and how far every
-- effect lets the picture recede into the paper (in sRGB: the room its
-- ink needs over the brightest of a picture in the dark, the darkest in
-- the light): one offset, the same for all.
M.PUSH = { dark = 2.2, light = 1.7 }
M.RECEDE = { dark = 0.07, light = 0.08 }

-- Which candidate each effect draws.
M.VARIANT = { dither = "silk", ascii = "between_mid", halftone = "tiles", scanlines = "crt" } :: { [string]: string }

local COMMON = string.format(
	[[
// params[0]: strength, density, light (1) or dark (0), faded (1) or not.
// params[1]: the picture's grid (its pixels across, down: 0 for no
//   picture) and where its overflow is cut (0 to 1, 0.5 centred).
// params[2]: its zoom, the ink's least and most share, the recede.
// params[3]: the push, the picture's tones (its lightness's 2nd and 98th
//   percentiles: 0 and 1 where they aren't known).
// The paper, by scheme (sRGB).
const FX_PAPER_DARK=vec3<f32>(%.4f,%.4f,%.4f);
const FX_PAPER_LIGHT=vec3<f32>(%.4f,%.4f,%.4f);
fn fx_paper_srgb(input:LayerInput)->vec3<f32> {return select(FX_PAPER_DARK,FX_PAPER_LIGHT,input.params[0].z>.5);}
// The picture's grid, covering the box: its top left in points, and a
// pixel's size (half a point without a picture).
fn fx_grid(input:LayerInput)->vec3<f32> {
 let pw=input.params[1].x; let ph=input.params[1].y;
 if pw<=0. || ph<=0. {return vec3(0.,0.,.5);}
 let s=max(input.size.x/pw,input.size.y/ph)*max(input.params[2].x,1.);
 return vec3(input.params[1].z*(input.size.x-pw*s),input.params[1].w*(input.size.y-ph*s),s);
}
fn fx_lin(c:vec3<f32>)->vec3<f32> {
 let x=clamp(c,vec3(0.),vec3(1.));
 return select(pow((x+.055)/1.055,vec3(2.4)),x/12.92,x<=vec3(.04045));
}
fn fx_srgb(c:vec3<f32>)->vec3<f32> {
 let x=clamp(c,vec3(0.),vec3(1.));
 return select(1.055*pow(x,vec3(1./2.4))-.055,x*12.92,x<=vec3(.0031308));
}
// The picture at `q` on its grid (in its pixels from its top left), linear,
// receded into the paper by the common offset (as the eye sees it: in
// sRGB, so the dark of a picture isn't lifted more than its light).
fn fx_src(input:LayerInput,g:vec3<f32>,q:vec2<f32>)->vec3<f32> {
 let c=zgx_sample(clamp(g.xy+q*g.z,vec2(0.),input.size-vec2(.01))).rgb;
 return fx_lin(mix(c,fx_paper_srgb(input),input.params[2].w));
}
// Its mean over a box `r` of its pixels across, centred at `q`.
fn fx_mean(input:LayerInput,g:vec3<f32>,q:vec2<f32>,r:vec2<f32>)->vec3<f32> {
 let d=r*.25;
 return .25*(fx_src(input,g,q-d)+fx_src(input,g,q+d)+fx_src(input,g,q+vec2(d.x,-d.y))+fx_src(input,g,q+vec2(-d.x,d.y)));
}
fn fx_paper(input:LayerInput)->vec3<f32> {return fx_lin(fx_paper_srgb(input));}
// How far `c` stands from the paper `p` towards the edge of the colours
// (0 the paper, 1 the edge): the least share of ink that makes `c` up.
fn fx_reach(c:vec3<f32>,p:vec3<f32>)->f32 {
 let up=(c-p)/max(vec3(1.)-p,vec3(1e-4));
 let dn=(p-c)/max(p,vec3(1e-4));
 let m=max(up,dn);
 return clamp(max(max(m.x,m.y),m.z),0.,1.);
}
struct FxInk { ink:vec3<f32>, gap:vec3<f32> }
// Ink covering `k` of a cell, and its gaps, that make `c` up: the ink on
// the line from the paper through `c` (as far as covering `k` takes, no
// further than the push, nor than the colours go), the gaps the paper or,
// where the ink can't make it up alone, the rest of `c` on that line.
fn fx_split(input:LayerInput,c:vec3<f32>,k:f32)->FxInk {return fx_split_by(input,c,k,input.params[3].x);}
fn fx_split_by(input:LayerInput,c:vec3<f32>,k:f32,push:f32)->FxInk {
 let p=fx_paper(input);
 let ink=p+(c-p)*min(1./max(max(k,fx_reach(c,p)),1e-4),push);
 return FxInk(ink,(c-k*ink)/max(1.-k,1e-4));
}
// How much of a cell its ink covers where the picture is `c`: from the
// least to the most share as `c` stands from the paper to the edge.
fn fx_share(input:LayerInput,c:vec3<f32>)->f32 {
 return mix(input.params[2].y,input.params[2].z,fx_reach(c,fx_paper(input)));
}
// How much of the art shows here: full at the top, nothing at the foot.
fn fx_alpha(input:LayerInput)->f32 {
 if input.params[0].w<.5 {return 1.;}
 return smoothstep(0.,input.size.y,input.size.y-input.p.y);
}
// What of the window's pixel at `x` (`w` wide, both in cells) falls in the
// next cell over: 0 when it's all in its own.
fn fx_over(x:vec2<f32>,w:f32)->vec2<f32> {
 let lo=x-.5*w;
 return clamp((x+.5*w-(floor(lo)+1.))/max(w,1e-4),vec2(0.),vec2(1.));
}
fn layer(input:LayerInput)->vec4<f32> {
 let g=fx_grid(input);
 let q=(input.p-g.xy)/g.z;
 // The window's pixel, in the picture's.
 let fp=1./(g.z*max(input.scale,1.));
 let e=fx_effect(input,g,q,fp);
 let rgb=fx_srgb(mix(fx_lin(input.color.rgb),e,input.params[0].x));
 return vec4(rgb,input.color.a*fx_alpha(input));
}
]],
	M.PAPER.dark[1],
	M.PAPER.dark[2],
	M.PAPER.dark[3],
	M.PAPER.light[1],
	M.PAPER.light[2],
	M.PAPER.light[3]
)

-- A cell-by-cell effect (`fx_cell(input, g, k)`, cells `unit` of the
-- picture's pixels a side): the window's pixel takes each cell it covers
-- by how much of it falls there.
local function cells(unit: number): string
	return string.format(
		[[
const FX_UNIT:f32=%.4f;
fn fx_effect(input:LayerInput,g:vec3<f32>,q:vec2<f32>,fp:f32)->vec3<f32> {
 let x=q/FX_UNIT; let w=fp/FX_UNIT;
 let k=vec2<i32>(floor(x-.5*w));
 let o=fx_over(x,w);
 var s=fx_cell(input,g,k)*(1.-o.x)*(1.-o.y);
 if o.x>0. {s+=fx_cell(input,g,k+vec2(1,0))*o.x*(1.-o.y);}
 if o.y>0. {s+=fx_cell(input,g,k+vec2(0,1))*(1.-o.x)*o.y;}
 if o.x>0. && o.y>0. {s+=fx_cell(input,g,k+vec2(1,1))*o.x*o.y;}
 return s;
}
]],
		unit
	)
end

-- Dither: every cell of `cell` of the picture's pixels quantized to one of
-- two shades of its own colour, a lighter and a darker, by Bayer's 8 by 8
-- screen: the lighter's share of the cells is the cell's lightness (so a
-- sky's mid tones and a shadow's both take the pattern), and the two shades
-- are `contrast` of as far apart as that share leaves room for (neither
-- past black or white), so the cell's mean is its colour (in sRGB). A
-- gamma `curve` before quantizing sets its brightness against None's.
local function dither(o: { cell: number, contrast: number, curve: number, pull: number }): string
	return cells(o.cell)
		.. string.format("const FX_CONTRAST:f32=%.3f;\nconst FX_CURVE:f32=%.3f;\nconst FX_PULL:f32=%.3f;\n", o.contrast, o.curve, o.pull)
		.. [[
fn fx_threshold(k:vec2<i32>)->f32 {
 let x=u32(k.x); let y=u32(k.y); var v=0u;
 for (var i=0u;i<3u;i++) {v|=(((((x^y)>>i)&1u)<<1u)|((y>>i)&1u))<<(4u-2u*i);}
 return (f32(v)+.5)/64.;
}
fn fx_cell(input:LayerInput,g:vec3<f32>,k:vec2<i32>)->vec3<f32> {
 // The cell's colour, in sRGB, through the curve.
 let c=pow(fx_srgb(fx_mean(input,g,(vec2<f32>(k)+.5)*FX_UNIT,vec2(FX_UNIT))),vec3(FX_CURVE));
 let t=fx_threshold(k);
 // (Pulled towards half by FX_PULL: a shadow takes a fine weave too.)
 let share=clamp(mix(dot(c,vec3(.2126,.7152,.0722)),.5,FX_PULL),.06,.94);
 // As far apart as the colours allow (neither shade past 0 or 1), times
 // the contrast: a shadow takes bright specks, a sky a full weave.
 let room=min((vec3(1.)-c)/(1.-share),c/share);
 let spread=FX_CONTRAST*min(min(room.x,room.y),room.z);
 return fx_lin(select(c-spread*share,c+spread*(1.-share),share>t));
}
]]
end

-- ASCII between Wing's (zgx's first: Zeron's ramp, cells in points, dark
-- cells left plain, glyphs at full value over the picture) and Zeron's own
-- (`original`): one shader, set by `o`:
--   px        a font pixel, in points (a cell is 6 by 8 of them), anchored
--             to the art's box (still as a sidebar or panel moves)
--   ramp      the glyphs, lightest first after a space
--   gamma     how the glyph rises with the cell's brightness
--   threshold cells darker than this hold no glyph
--   invert    in the light, the glyph by the cell's darkness (Zeron's)
--   vivid     the glyph's colour, from the cell's own (0) to full value (1)
--   lift      the glyph lifted towards white (in the light, black)
--   glyph     how much the art lies over the picture
--   ground    between the strokes: the paper (0) or the picture dimmed (1)
-- All in sRGB, as both were.
local WING_RAMP = {
	{ 0, 0, 0, 0, 0, 12, 12 }, -- "."
	{ 0, 0, 0, 31, 0, 0, 0 }, -- "-"
	{ 0, 4, 4, 31, 4, 4, 0 }, -- "+"
	{ 0, 0, 17, 10, 4, 10, 17 }, -- "x"
	{ 17, 17, 10, 4, 10, 17, 17 }, -- "X"
	{ 15, 16, 16, 14, 1, 1, 30 }, -- "S"
	{ 14, 17, 17, 14, 17, 17, 14 }, -- "8"
	{ 14, 17, 19, 21, 25, 17, 14 }, -- "0"
	{ 10, 10, 31, 10, 31, 10, 10 }, -- "#"
	{ 14, 17, 23, 21, 23, 16, 15 }, -- "@"
}
-- Zeron's own, ` .:-=+*#%@`.
local ORIGINAL_RAMP = {
	{ 0, 0, 0, 0, 0, 4, 0 }, -- "."
	{ 0, 4, 0, 0, 4, 0, 0 }, -- ":"
	{ 0, 0, 0, 14, 0, 0, 0 }, -- "-"
	{ 0, 0, 14, 0, 14, 0, 0 }, -- "="
	{ 0, 4, 4, 31, 4, 4, 0 }, -- "+"
	{ 0, 21, 14, 31, 14, 21, 0 }, -- "*"
	{ 10, 10, 31, 10, 31, 10, 10 }, -- "#"
	{ 17, 2, 4, 4, 8, 16, 17 }, -- "%"
	{ 14, 17, 23, 21, 23, 16, 14 }, -- "@"
}
-- Zeron's own less `%` (a diagonal, it hatches).
local CLEAN_RAMP = {
	{ 0, 0, 0, 0, 0, 4, 0 }, -- "."
	{ 0, 4, 0, 0, 4, 0, 0 }, -- ":"
	{ 0, 0, 0, 14, 0, 0, 0 }, -- "-"
	{ 0, 0, 14, 0, 14, 0, 0 }, -- "="
	{ 0, 4, 4, 31, 4, 4, 0 }, -- "+"
	{ 0, 21, 14, 31, 14, 21, 0 }, -- "*"
	{ 10, 10, 31, 10, 31, 10, 10 }, -- "#"
	{ 14, 17, 23, 21, 23, 16, 14 }, -- "@"
}
type Blend = {
	px: number,
	ramp: { { number } },
	gamma: number,
	threshold: number,
	invert: boolean,
	vivid: number,
	lift: number,
	glyph: number,
	ground: number,
	dim: number,
}
local function blend(o: Blend): string
	local rows = { "0u", "0u", "0u", "0u", "0u", "0u", "0u" }
	for _, g in o.ramp do
		for _, r in g do
			table.insert(rows, `{r}u`)
		end
	end
	return string.format(
		[[
const FX_PX:f32=%.4f;
const FX_RAMP=array<u32,%d>(%s);
const FX_N:u32=%du;
const FX_GAMMA:f32=%.3f;
const FX_T:f32=%.3f;
const FX_INVERT:bool=%s;
const FX_VIVID:f32=%.3f;
const FX_LIFT:f32=%.3f;
const FX_K:f32=%.3f;
const FX_GROUND:f32=%.3f;
const FX_DIM:f32=%.3f;
]],
		o.px,
		#rows,
		table.concat(rows, ","),
		#o.ramp + 1,
		o.gamma,
		o.threshold,
		if o.invert then "true" else "false",
		o.vivid,
		o.lift,
		o.glyph,
		o.ground,
		o.dim
	) .. [[
fn fx_b_at(input:LayerInput,p:vec2<f32>)->vec3<f32> {return zgx_sample(clamp(p,vec2(0.),input.size-vec2(.01))).rgb;}
// Font pixel `k`'s colour: its cell's glyph stroke, or the ground.
fn fx_b_pix(input:LayerInput,k:vec2<i32>)->vec3<f32> {
 let light=input.params[0].z>.5;
 let cell=vec2<i32>(floor(vec2<f32>(k)/vec2(6.,8.)));
 let c=fx_b_at(input,(vec2<f32>(cell)+.5)*vec2(6.,8.)*FX_PX);
 var lum=dot(c,vec3(.2126,.7152,.0722));
 if FX_INVERT && light {lum=1.-lum;}
 let level=pow(clamp((lum-FX_T)/max(1.-FX_T,.01),0.,1.),FX_GAMMA);
 let i=min(u32(level*(f32(FX_N)-.001)),FX_N-1u);
 let at=k-cell*vec2(6,8);
 var lit=false;
 if i>0u && at.x<5 && at.y<7 {lit=((FX_RAMP[i*7u+u32(at.y)]>>u32(4-at.x))&1u)==1u;}
 if lit {
  let peak=max(max(c.r,c.g),max(c.b,.004));
  let g=mix(c,c/peak,FX_VIVID);
  let to=select(vec3(1.),vec3(0.),light && FX_INVERT);
  return mix(g,to,FX_LIFT);
 }
 let here=fx_b_at(input,(vec2<f32>(k)+.5)*FX_PX);
 return mix(fx_paper_srgb(input),here*FX_DIM,FX_GROUND);
}
fn fx_effect(input:LayerInput,g:vec3<f32>,q:vec2<f32>,fp:f32)->vec3<f32> {
 let x=input.p/FX_PX; let w=1./(max(input.scale,1.)*FX_PX);
 let k=vec2<i32>(floor(x-.5*w));
 let o=fx_over(x,w);
 var s=fx_b_pix(input,k)*(1.-o.x)*(1.-o.y);
 if o.x>0. {s+=fx_b_pix(input,k+vec2(1,0))*o.x*(1.-o.y);}
 if o.y>0. {s+=fx_b_pix(input,k+vec2(0,1))*(1.-o.x)*o.y;}
 if o.x>0. && o.y>0. {s+=fx_b_pix(input,k+vec2(1,1))*o.x*o.y;}
 return fx_lin(mix(input.color.rgb,s,FX_K));
}
]]
end

-- Zeron's ASCII as it was (crates/ui/src/new_thread_background_effects.rs
-- `ascii_pixels`), with none of the shared tone: cells of 6 by 8 of the
-- picture's own pixels, each the glyph of ` .:-=+*#%@` whose ink is the
-- brightness at its centre (in the light, the darkness), square-rooted,
-- lit in the centre's colour, over the picture at 60% on its paper (black
-- in the dark, white in the light). All in sRGB, as Zeron's bytes were,
-- and the window's pixel the blend of the four picture pixels round it, as
-- a picture's filtering blends them.
local ORIGINAL = [[
fn fx_o_src(input:LayerInput,g:vec3<f32>,i:vec2<u32>)->vec3<f32> {
 return zgx_sample(clamp(g.xy+(vec2<f32>(i)+.5)*g.z,vec2(0.),input.size-vec2(.01))).rgb;
}
fn fx_o_glyph(i:u32,row:u32)->u32 {
 var g=array<u32,7>(0u,0u,0u,0u,0u,0u,0u);
 switch i {
  case 1u: {g=array<u32,7>(0u,0u,0u,0u,0u,4u,0u);}
  case 2u: {g=array<u32,7>(0u,4u,0u,0u,4u,0u,0u);}
  case 3u: {g=array<u32,7>(0u,0u,0u,14u,0u,0u,0u);}
  case 4u: {g=array<u32,7>(0u,0u,14u,0u,14u,0u,0u);}
  case 5u: {g=array<u32,7>(0u,4u,4u,31u,4u,4u,0u);}
  case 6u: {g=array<u32,7>(0u,21u,14u,31u,14u,21u,0u);}
  case 7u: {g=array<u32,7>(10u,10u,31u,10u,31u,10u,10u);}
  case 8u: {g=array<u32,7>(17u,2u,4u,4u,8u,16u,17u);}
  case 9u: {g=array<u32,7>(14u,17u,23u,21u,23u,16u,14u);}
  default: {}
 }
 return g[min(row,6u)];
}
fn fx_o_pixel(input:LayerInput,g:vec3<f32>,i:vec2<u32>)->vec3<f32> {
 let light=input.params[0].z>.5;
 let at=vec2(i.x/6u*6u+3u,i.y/8u*8u+4u);
 let gc=fx_o_src(input,g,at);
 let l=dot(gc,vec3(.2126,.7152,.0722));
 let t=input.params[0].y;
 let ink=clamp((select(l,1.-l,light)-t)/max(1.-t,.01),0.,1.);
 let k=min(u32(sqrt(ink)*9.),9u);
 let cx=i.x%6u; let cy=i.y%8u;
 var lit=false;
 if cx<5u && cy<7u {lit=((fx_o_glyph(k,cy)>>(4u-cx))&1u)==1u;}
 let paper=select(vec3(0.),vec3(1.),light);
 return fx_o_src(input,g,i)*.6+select(paper,gc,lit)*.4;
}
fn fx_effect(input:LayerInput,g:vec3<f32>,q:vec2<f32>,fp:f32)->vec3<f32> {
 let u=q-.5;
 let f=fract(u);
 let i0=vec2<u32>(max(vec2<i32>(floor(u)),vec2(0)));
 let e=mix(mix(fx_o_pixel(input,g,i0),fx_o_pixel(input,g,i0+vec2(1u,0u)),f.x),
  mix(fx_o_pixel(input,g,i0+vec2(0u,1u)),fx_o_pixel(input,g,i0+vec2(1u,1u)),f.x),f.y);
 return fx_lin(e);
}
]]

-- Halftone: a dot in each cell of a screen `pitch` pixels apart at `angle`
-- degrees, its area what the colour there takes: round (growing into a
-- chain, then leaving holes) or a rounded square, as Zeron's mark's cells.
local function halftone(o: { pitch: number, angle: number, square: boolean }): string
	local a = math.rad(o.angle)
	return string.format(
		[[
const FX_PITCH:f32=%.4f;
const FX_ROT=mat2x2<f32>(%.6f,%.6f,%.6f,%.6f);
const FX_SQUARE:bool=%s;
// A dot's size for a share `k` of its cell (in cells): a round dot's
// radius (clipped by the cell past a half), a square's half side.
fn fx_size(k:f32)->f32 {
 if FX_SQUARE {return .5*sqrt(k/.9657);}
 if k<=.785398 {return sqrt(k/3.14159265);}
 var r=mix(.5,.70711,(k-.785398)/.214602);
 for (var n=0;n<4;n++) {
  let a=acos(min(.5/r,1.));
  let area=3.14159265*r*r-4.*(r*r*a-.5*sqrt(max(r*r-.25,0.)));
  r=clamp(r-(area-k)/max(2.*r*(3.14159265-4.*a),1e-3),.5,.70711);
 }
 return r;
}
// How far `x` (in cells, from the dot's centre) is outside a dot of size `r`.
fn fx_dot(x:vec2<f32>,r:f32)->f32 {
 if FX_SQUARE {let c=.4*r; return length(max(abs(x)-vec2(r-c),vec2(0.)))-c;}
 return length(x)-r;
}
fn fx_effect(input:LayerInput,g:vec3<f32>,q:vec2<f32>,fp:f32)->vec3<f32> {
 let s=FX_ROT*q/FX_PITCH;
 let near=floor(s);
 let b=floor(s-.5);
 var ink=FxInk(vec3(0.),vec3(0.));
 var cover=0.;
 for (var j=0.;j<2.;j+=1.) {for (var i=0.;i<2.;i+=1.) {
  let k=b+vec2(i,j);
  let c=fx_mean(input,g,transpose(FX_ROT)*((k+.5)*FX_PITCH),vec2(FX_PITCH*.7));
  let share=fx_share(input,c);
  let d=fx_dot(s-(k+.5),fx_size(share));
  cover=max(cover,clamp(.5-d*FX_PITCH/fp,0.,1.));
  if all(k==near) {ink=fx_split(input,c,share);}
 }}
 return mix(ink.gap,ink.ink,cover);
}
]],
		o.pitch,
		math.cos(a),
		math.sin(a),
		-math.sin(a),
		math.cos(a),
		if o.square then "true" else "false"
	)
end

-- Scanlines: rows `period` pixels apart, each a band of ink across the
-- paper: a CRT's (a fixed `band` of each row lit, soft at its edges, the
-- dark between lifting where the picture is bright) or a line screen's (the
-- band as thick as the colour there takes, sharp).
local function scanlines(o: { period: number, band: number?, soft: number }): string
	return string.format(
		[[
const FX_PERIOD:f32=%.4f;
const FX_BAND:f32=%.4f;
const FX_SOFT:f32=%.4f;
fn fx_effect(input:LayerInput,g:vec3<f32>,q:vec2<f32>,fp:f32)->vec3<f32> {
 let y=q.y/FX_PERIOD;
 let row=floor(y);
 let c=fx_mean(input,g,vec2(q.x,(row+.5)*FX_PERIOD),vec2(FX_PERIOD*.5,FX_PERIOD));
 let share=select(fx_share(input,c),FX_BAND,FX_BAND>0.);
 let ink=fx_split(input,c,share);
 // The band, centred in its row, its edge over the window's pixel (and
 // softened by FX_SOFT of a row).
 let d=(abs(y-row-.5)-.5*share)*FX_PERIOD;
 let edge=max(fp,FX_SOFT*FX_PERIOD);
 return mix(ink.gap,ink.ink,clamp(.5-d/edge,0.,1.));
}
]],
		o.period,
		o.band or 0,
		o.soft
	)
end

-- Every candidate, by effect: what each draws.
M.VARIANTS = {
	dither = {
		-- Two shades of each cell's colour, cells of 2 points.
		silk = function()
			return dither { cell = 2.5, contrast = 0.55, curve = 1.06, pull = 0.3 }
		end,
		-- The same, finer: cells of 1.5 points.
		fine = function()
			return dither { cell = 1.9, contrast = 0.55, curve = 1.06, pull = 0.3 }
		end,
		-- The shades further apart: closer to Zeron's lit-or-dim cells.
		bold = function()
			return dither { cell = 2.5, contrast = 0.85, curve = 1.06, pull = 0.15 }
		end,
	},
	ascii = {
		-- Halfway on every axis: a font pixel of 1.1 points, Zeron's ramp
		-- less `%`, light sparseness, the cell's colour a little lifted, 68%
		-- picture, on half paper half picture.
		between_mid = function()
			return blend { px = 1.1, ramp = CLEAN_RAMP, gamma = 0.8, threshold = 0.1, invert = true, vivid = 0.5, lift = 0.22, glyph = 0.34, ground = 0.3, dim = 0.75 }
		end,
		-- Nearer Wing's: his ramp, dark cells left plain, a font pixel of
		-- 1.25 points, colours between the cell's own and full value.
		between_wing = function()
			return blend { px = 1.25, ramp = WING_RAMP, gamma = 1.2, threshold = 0.2, invert = false, vivid = 0.5, lift = 0, glyph = 0.65, ground = 1, dim = 0.75 }
		end,
		-- Nearer Zeron's own: every cell a glyph, its ramp and 60/40 mix on
		-- the paper, in crisp cells of 1-point font pixels, the glyphs lifted.
		between_original = function()
			return blend { px = 1, ramp = ORIGINAL_RAMP, gamma = 0.5, threshold = 0, invert = true, vivid = 0, lift = 0.15, glyph = 0.4, ground = 0, dim = 1 }
		end,
		-- Zeron's own, as it was (Gaël's).
		original = function()
			return ORIGINAL
		end,
		-- Wing's, as zgx first drew it.
		wing = function()
			return blend { px = 1.5, ramp = WING_RAMP, gamma = 1.4, threshold = 0.22, invert = false, vivid = 1, lift = 0, glyph = 0.75, ground = 1, dim = 0.75 }
		end,
	},
	halftone = {
		-- Round dots at 45°, the press's screen.
		round = function()
			return halftone { pitch = 5, angle = 45, square = false }
		end,
		-- Rounded squares on the grid, as the cells of Zeron's mark.
		tiles = function()
			return halftone { pitch = 7, angle = 0, square = true }
		end,
		-- Finer round dots at 15°.
		fine = function()
			return halftone { pitch = 3.5, angle = 15, square = false }
		end,
	},
	scanlines = {
		-- A CRT's rows, 4 of the picture's pixels apart: 55% of each lit, soft.
		crt = function()
			return scanlines { period = 4, band = 0.55, soft = 0.25 }
		end,
		-- A line screen: each row's band as thick as its colour takes.
		lines = function()
			return scanlines { period = 3.5, soft = 0 }
		end,
		-- Fine CRT rows, sharper.
		fine = function()
			return scanlines { period = 2, band = 0.55, soft = 0.1 }
		end,
	},
} :: { [string]: { [string]: () -> string } }

local NONE = [[
fn fx_effect(input:LayerInput,g:vec3<f32>,q:vec2<f32>,fp:f32)->vec3<f32> {return fx_lin(input.color.rgb);}
]]

local made: { [string]: any } = {}
-- The layer shader for an effect ("dither", or a candidate: "dither/silk"),
-- made once.
function M.shader(name: string): any
	local e, v = string.match(name, "^([^/]*)/?(.*)$")
	local effect: string, variant: string = e or "none", v or ""
	local of = M.VARIANTS[effect]
	if not of then
		effect, variant = "none", ""
	elseif variant == "" or not of[variant] then
		variant = M.VARIANT[effect] or ""
	end
	local key = `{effect}/{variant}`
	if not made[key] then
		local make = if of then of[variant] else nil
		made[key] = ui.layer_shader(COMMON .. (if make then make() else NONE))
	end
	return made[key]
end

export type Grid = { w: number, h: number, x: number?, y: number?, zoom: number? }
export type Params = {
	strength: number?,
	threshold: number?,
	light: boolean?,
	faded: boolean?,
	picture: Grid?,
	-- The picture's tones (`image.tones`: its lightness's 2nd and 98th
	-- percentiles), where known.
	tones: { lo: number, hi: number }?,
}

-- The picture's grid: its pixels as Zeron's proxy of it has them (the
-- picture fitted to 2048 a side, made larger as well as smaller).
function M.grid(p: Grid?): (number, number)
	if not p or p.w <= 0 or p.h <= 0 then
		return 0, 0
	end
	local k = M.GRID_MAX / math.max(p.w, p.h)
	return math.max(1, math.round(p.w * k)), math.max(1, math.round(p.h * k))
end

-- The layer's parameters (`layer_params`): `layerParam0` the strength, 1
-- the density, 3 faded, 13 and 14 the tones, to set in place.
function M.params(o: Params): { any }
	local p = o.picture
	local gw, gh = M.grid(p)
	local scheme = if o.light then "light" else "dark"
	local tones = o.tones or { lo = 0, hi = 1 }
	return {
		{ o.strength or 1, o.threshold or 0, if o.light then 1 else 0, if o.faded == false then 0 else 1 },
		{ gw, gh, if p then math.clamp(p.x or 0.5, 0, 1) else 0.5, if p then math.clamp(p.y or 0.5, 0, 1) else 0.5 },
		{ if p then p.zoom or 1 else 1, M.FLOOR, M.MOST, M.RECEDE[scheme] },
		{ M.PUSH[scheme], tones.lo, tones.hi, 0 },
	}
end

return M
