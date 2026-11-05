-- A new chat's wallpaper effects, as Zeron's draws them (crates/ui/src/
-- new_thread_background_effects.rs and new_thread_background_mask.rs): each
-- a layer shader over the whole picture as it's composited, so the art in
-- it (a picture, or what a plugin draws) takes one pass, not one a layer.
--
-- `ui.overlay { layer_shader = effects.SHADERS.dither, layer_params = effects.params { … }, … }`
--
-- Zeron drew its effects on the CPU into the picture's own pixels (the
-- picture fitted to 2048 a side first), then laid that
-- picture over the window, scaled to cover the new chat's art and filtered
-- as a picture is. These do the same on the GPU: the effect is worked out
-- for each of the picture's pixels, on its grid (so it grows and shrinks
-- with the picture, the dots of a halftone a few of its pixels apart at any
-- window size), and the window's pixel between four of them takes their
-- blend, as a picture's filtering gives. The treatments are Zeron's:
--
-- * Dither: a 4×4 Bayer screen in cells of 2 by 2 of the picture's pixels,
--   each lit in its colour at full value where its brightest channel
--   passes the cell's threshold, else that colour at 8%.
-- * ASCII: cells of 6 by 8 pixels, each the glyph of ` .:-=+*#%@` whose
--   ink is the brightness there (in the light, the darkness), its strokes
--   in the cell's colour, over the picture at 60% on its paper (black in
--   the dark, white in the light).
-- * Halftone: a dot in each 4 by 4 cell, its radius by the brightness (the
--   darkness in the light), in the cell's colour on its paper, over the
--   picture at 60%.
-- * Scanlines: every third row of the picture's pixels darkened to 52% (in
--   the light, lifted that much towards white).
--
-- And the mask Zeron laid them with: the art is faded from its top, full,
-- to nothing at its foot (a smoothstep over its height), and cleared round
-- the composer (a rounded rectangle from its top down to the art's foot,
-- feathered over half the art's height, between 120 and 280), showing half
-- as strongly there; all of it at `opacity` (Zeron's frosted window took
-- 84%).

local M = {}

-- How much of its box a new chat's art takes from the top, and at most
-- (Zeron's: 72% of the window's height, 760 at most).
M.HERO_SHARE, M.HERO_MAX = 0.72, 760
-- The longer side of the picture's grid (Zeron's proxy of the picture).
M.GRID_MAX = 2048

local COMMON = [[
const HERO_SHARE: f32 = ]] .. M.HERO_SHARE .. [[;
const HERO_MAX: f32 = ]] .. M.HERO_MAX .. [[;
// params[0]: strength, threshold, light (1) or dark (0), opacity.
// params[1]: the picture's grid (its pixels across, down: 0 for no picture)
//   and where its overflow is cut (0 to 1, 0.5 centred).
// params[2]: its zoom, the hero (1: laid in the top of the box and faded;
//   0: the whole box), the clearing's middle from the box's (x, y).
// params[3]: the clearing's width at most, the room it leaves (its width
//   is at most the box's less this), its height, its corner.
fn fx_hero_h(input:LayerInput)->f32 {
 return select(input.size.y,min(input.size.y*HERO_SHARE,HERO_MAX),input.params[2].y>.5);
}
// The picture's grid: its top left in points, and a pixel's size.
fn fx_grid(input:LayerInput)->vec3<f32> {
 let pw=input.params[1].x; let ph=input.params[1].y;
 if pw<=0. || ph<=0. {return vec3(0.,0.,.5);}
 let hh=fx_hero_h(input);
 let s=max(input.size.x/pw,hh/ph)*max(input.params[2].x,1.);
 return vec3(input.params[1].z*(input.size.x-pw*s),input.params[1].w*(hh-ph*s),s);
}
// The picture's pixel `i` (whole, from its top left).
fn fx_src(input:LayerInput,g:vec3<f32>,i:vec2<u32>)->vec3<f32> {
 return zgx_sample(g.xy+(vec2<f32>(i)+.5)*g.z).rgb;
}
fn fx_luma(c:vec3<f32>)->f32 {return dot(c,vec3(.2126,.7152,.0722));}
fn fx_light(input:LayerInput)->bool {return input.params[0].z>.5;}
fn fx_paper(input:LayerInput)->vec3<f32> {return select(vec3(0.),vec3(1.),fx_light(input));}
// How much of the art shows here: Zeron's two passes, the cleared one over
// one at half strength, both faded down the art's height.
fn fx_alpha(input:LayerInput)->f32 {
 var a=1.;
 if input.params[2].y>.5 {
  let hh=fx_hero_h(input);
  let b=smoothstep(0.,hh,hh-input.p.y);
  var m=1.;
  let w=min(input.params[3].x,input.size.x-input.params[3].y);
  if input.params[3].x>0. && w>0. {
   let c=input.size*.5+input.params[2].zw;
   let top=c.y-input.params[3].z*.5;
   let bottom=max(c.y+input.params[3].z*.5,hh);
   let half=vec2(w,bottom-top)*.5;
   let r=min(input.params[3].w,min(half.x,half.y));
   let q=abs(input.p-vec2(c.x,(top+bottom)*.5))-half+r;
   let d=length(max(q,vec2(0.)))+min(max(q.x,q.y),0.)-r;
   m=smoothstep(0.,clamp(hh*.52,120.,280.),d-8.);
  }
  a=1.-(1.-.5*b)*(1.-m*b);
 }
 return a*input.params[0].w;
}
]]

local function shader(effect: string): any
	return ui.layer_shader(COMMON .. effect .. [[
// The effect at the window's pixel: worked out at the four of the
// picture's pixels round it, blended as a picture's filtering blends them.
fn layer(input:LayerInput)->vec4<f32> {
 let g=fx_grid(input);
 let u=(input.p-g.xy)/g.z-.5;
 let f=fract(u);
 let i0=vec2<u32>(max(vec2<i32>(floor(u)),vec2(0)));
 let e=mix(mix(fx_effect(input,g,i0),fx_effect(input,g,i0+vec2(1u,0u)),f.x),
  mix(fx_effect(input,g,i0+vec2(0u,1u)),fx_effect(input,g,i0+vec2(1u,1u)),f.x),f.y);
 let rgb=clamp(mix(input.color.rgb,e,input.params[0].x),vec3(0.),vec3(1.));
 return vec4(rgb,input.color.a*fx_alpha(input));
}
]])
end

M.SHADERS = {
	-- The picture as it is, laid with the mask.
	none = ui.layer_shader(COMMON .. [[
fn layer(input:LayerInput)->vec4<f32> {
 return vec4(input.color.rgb,input.color.a*fx_alpha(input));
}
]]),
	dither = shader([[
fn fx_effect(input:LayerInput,g:vec3<f32>,i:vec2<u32>)->vec3<f32> {
 var bayer=array<u32,16>(0u,8u,2u,10u,12u,4u,14u,6u,3u,11u,1u,9u,15u,7u,13u,5u);
 let b=i/2u;
 let c=fx_src(input,g,b*2u+1u);
 let t=f32(bayer[(b.y%4u)*4u+b.x%4u]);
 let peak=max(max(c.r,c.g),c.b);
 return select(c*.08,c/max(peak,1./255.),peak>(t+.5)/16.);
}
]]),
	ascii = shader([[
// Rows of five, top to bottom: ` .:-=+*#%@`.
fn fx_glyph(i:u32,row:u32)->u32 {
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
fn fx_effect(input:LayerInput,g:vec3<f32>,i:vec2<u32>)->vec3<f32> {
 let at=vec2(i.x/6u*6u+3u,i.y/8u*8u+4u);
 let gc=fx_src(input,g,at);
 let l=fx_luma(gc);
 let t=input.params[0].y;
 let ink=clamp((select(l,1.-l,fx_light(input))-t)/max(1.-t,.01),0.,1.);
 let k=min(u32(sqrt(ink)*9.),9u);
 let cx=i.x%6u; let cy=i.y%8u;
 var lit=false;
 if cx<5u && cy<7u {lit=((fx_glyph(k,cy)>>(4u-cx))&1u)==1u;}
 return fx_src(input,g,i)*.6+select(fx_paper(input),gc,lit)*.4;
}
]]),
	halftone = shader([[
fn fx_effect(input:LayerInput,g:vec3<f32>,i:vec2<u32>)->vec3<f32> {
 let b=i/4u*4u;
 let l=fx_luma(fx_src(input,g,b));
 let tone=select(l,1.-l,fx_light(input));
 let radius=2.*(.3+.7*sqrt(tone));
 let cover=clamp(radius+.5-length(vec2<f32>(i-b)-1.5),0.,1.);
 let dot=fx_src(input,g,b+2u);
 return fx_src(input,g,i)*.6+mix(fx_paper(input),dot,cover)*.4;
}
]]),
	scanlines = shader([[
fn fx_effect(input:LayerInput,g:vec3<f32>,i:vec2<u32>)->vec3<f32> {
 let c=fx_src(input,g,i);
 let gain=select(1.,.52,i.y%3u==0u);
 return select(c*gain,c+(vec3(1.)-c)*(1.-gain),fx_light(input));
}
]]),
}

export type Grid = { w: number, h: number, x: number?, y: number?, zoom: number? }
export type Clear = { cx: number, cy: number, w: number, room: number, h: number, radius: number }
export type Params = {
	strength: number?,
	threshold: number?,
	light: boolean?,
	opacity: number?,
	picture: Grid?,
	hero: boolean?,
	clear: Clear?,
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

-- The layer's parameters (`layer_params`), each a `layerParam<n>` to set
-- in place: 0 the strength, 1 the threshold, 3 the opacity, 10 to 14 the
-- clearing.
function M.params(o: Params): { any }
	local p = o.picture
	local gw, gh = M.grid(p)
	local c = o.clear
	return {
		{ o.strength or 1, o.threshold or 0, if o.light then 1 else 0, o.opacity or 1 },
		{ gw, gh, if p then math.clamp(p.x or 0.5, 0, 1) else 0.5, if p then math.clamp(p.y or 0.5, 0, 1) else 0.5 },
		{ if p then p.zoom or 1 else 1, if o.hero == false then 0 else 1, if c then c.cx else 0, if c then c.cy else 0 },
		{ if c then c.w else 0, if c then c.room else 0, if c then c.h else 0, if c then c.radius else 0 },
	}
end

return M
