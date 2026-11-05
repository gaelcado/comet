-- Effects over a whole picture, as it is composited (so a scene of layers
-- takes one, not a layer each: transparent edges would dither into fringes
-- and the layers' grids into noise).
--
-- `ui.overlay { layer_shader = effects.dither, layer_params = effects.dither_params(), … }`
--
-- What they share, after Zeron's own (crates/ui/src/
-- new_thread_background_effects.rs), which drew them on the CPU into the
-- picture's pixels:
--
-- * Cells are whole device pixels, on a grid laid from the box's top right
--   corner (where a wallpaper is laid from), so they stay crisp at any
--   scale and hold still as a sidebar comes and goes.
-- * A cell takes the mean of the picture across it, not the one pixel at
--   its middle: fine detail (leaves, grain) doesn't flicker into noise.
-- * Tone is kept in linear light: what a cell's dots or lines give the eye,
--   seen from where one reads, is the picture's own brightness, so a
--   highlight stays a highlight and a mid tone neither bleaches nor muds.

local M = {}

-- The helpers every effect reads (WGSL).
local COMMON = [[
fn fx_lin(c:vec3<f32>)->vec3<f32> {
 return select(c/12.92,pow((c+.055)/1.055,vec3(2.4)),c>vec3(.04045));
}
fn fx_srgb(l:vec3<f32>)->vec3<f32> {
 let x=clamp(l,vec3(0.),vec3(1.));
 return select(x*12.92,1.055*pow(x,vec3(1./2.4))-.055,x>vec3(.0031308));
}
// The device pixel under p, from the box's top right (x counts down, left).
fn fx_px(input:LayerInput)->vec2<f32> {
 return vec2(input.p.x-input.size.x,input.p.y)*input.scale;
}
// The picture at device pixel d (on that grid), in points.
fn fx_at(input:LayerInput,d:vec2<f32>)->vec4<f32> {
 return zgx_sample(vec2(d.x/input.scale+input.size.x,d.y/input.scale));
}
// The picture's mean across the cell from device pixel o, s across: four
// filtered taps, each the mean of the texels round it (exact for 2 and 4).
fn fx_mean(input:LayerInput,o:vec2<f32>,s:vec2<f32>)->vec4<f32> {
 let q=s*.25;
 return (fx_at(input,o+q)+fx_at(input,o+vec2(3.*q.x,q.y))+fx_at(input,o+vec2(q.x,3.*q.y))+fx_at(input,o+3.*q))*.25;
}
// A cell's own whole numbers (a negative cell wraps, as a grid's should).
fn fx_cell(c:vec2<f32>)->vec2<u32> {
 return bitcast<vec2<u32>>(vec2<i32>(c));
}
]]

-- Dithered as Zeron dithers its wallpapers (`dither_pixels`): square cells
-- `cell` points across, each lit in the picture's colour at its full value
-- (its brightest channel at 1) where that channel passes the cell's
-- threshold, else that colour at 8%. Zeron's 4×4 Bayer screen is 8×8 here
-- (64 levels, so a sky's gradient doesn't band), and a cell is lit by its
-- peak in linear light: the share of cells lit is the light the picture
-- has there, where comparing the encoded value lit too many (mid tones
-- came out bleached). Laid over the picture at `strength`.
-- params[0]: the cell (points), the strength (0 to 1).
M.dither = ui.layer_shader(COMMON .. [[
fn fx_bayer8(c:vec2<u32>)->f32 {
 let x=c.x&7u; let z=x^(c.y&7u);
 let v=((z&1u)<<5u)|((x&1u)<<4u)|((z&2u)<<2u)|((x&2u)<<1u)|((z&4u)>>1u)|((x&4u)>>2u);
 return (f32(v)+.5)/64.;
}
fn layer(input:LayerInput)->vec4<f32> {
 let s=max(1.,round(input.params[0].x*input.scale));
 let cell=floor(fx_px(input)/s);
 let c=fx_mean(input,cell*s,vec2(s)).rgb;
 let peak=max(max(c.r,c.g),c.b);
 let lit=fx_lin(vec3(peak)).x>fx_bayer8(fx_cell(cell));
 let lit_c=select(c*.08,c/max(peak,.004),lit);
 return vec4(mix(input.color.rgb,lit_c,input.params[0].y),input.color.a);
}
]])

-- The dither's parameters: cells `cell` points across (1: Zeron's two
-- source pixels, as its wallpapers showed), the whole picture (`strength`
-- 1, as Zeron's).
function M.dither_params(cell: number?, strength: number?): { any }
	return { { cell or 1, strength or 1, 0, 0 }, { 0, 0, 0, 0 }, { 0, 0, 0, 0 }, { 0, 0, 0, 0 } }
end

-- Drawn in characters, as Zeron's (`ascii_pixels`): cells of a 5 by 7
-- font (a font pixel `px` points, a cell 6 by 8 of them), each the glyph of
-- ` .:-=+*%#@` whose ink matches the brightness across it (in the light,
-- its darkness), its strokes in the cell's colour: lifted towards its full
-- value over a dimmed picture in the dark, deepened over a paler one in
-- the light. Cells darker (lighter) than `threshold` hold none. Laid over
-- the picture at `strength`.
-- params[0]: the font pixel (points), the strength (0 to 1), the threshold
-- (0 to 1), light (1) or dark (0).
M.ascii = ui.layer_shader(COMMON .. [[
// Rows of five, top to bottom: ink rising 0, 1, 2, 3, 6, 9, 11, 13, 20, 21.
fn fx_glyph(i:u32,row:u32)->u32 {
 var g=array<u32,7>(0u,0u,0u,0u,0u,0u,0u);
 switch i {
  case 1u: {g=array<u32,7>(0u,0u,0u,0u,0u,4u,0u);}
  case 2u: {g=array<u32,7>(0u,4u,0u,0u,4u,0u,0u);}
  case 3u: {g=array<u32,7>(0u,0u,0u,14u,0u,0u,0u);}
  case 4u: {g=array<u32,7>(0u,0u,14u,0u,14u,0u,0u);}
  case 5u: {g=array<u32,7>(0u,4u,4u,31u,4u,4u,0u);}
  case 6u: {g=array<u32,7>(0u,4u,21u,14u,21u,4u,0u);}
  case 7u: {g=array<u32,7>(24u,25u,2u,4u,8u,19u,3u);}
  case 8u: {g=array<u32,7>(10u,10u,31u,10u,31u,10u,10u);}
  case 9u: {g=array<u32,7>(14u,17u,23u,21u,23u,16u,15u);}
  default: {}
 }
 return g[min(row,6u)];
}
fn layer(input:LayerInput)->vec4<f32> {
 let px=max(1.,round(input.params[0].x*input.scale));
 let size=vec2(6.,8.)*px;
 let d=fx_px(input);
 let cell=floor(d/size);
 let o=cell*size;
 // (Nine taps: a cell is wide.)
 var sum=vec4(0.);
 for (var j=0;j<3;j++) {for (var i=0;i<3;i++) {
  sum+=fx_at(input,o+size*vec2((f32(i)+.5)/3.,(f32(j)+.5)/3.));
 }}
 let c=(sum/9.).rgb;
 let light=input.params[0].w>.5;
 let luma=dot(c,vec3(.2126,.7152,.0722));
 let t=input.params[0].z;
 let tone=select(luma,1.-luma,light);
 let level=clamp((tone-t)/max(1.-t,.01),0.,1.);
 let i=u32(round(pow(level,.75)*9.));
 let fp=vec2<u32>(floor((d-o)/px));
 var lit=false;
 if i>0u && fp.x<5u && fp.y<7u {lit=((fx_glyph(i,fp.y)>>(4u-fp.x))&1u)==1u;}
 let peak=max(max(c.r,c.g),c.b);
 let own=input.color.rgb;
 // The dark: the picture dimmed (to about a third of its light in the
 // shadows, less towards its highlights, so a sky keeps its glow) under
 // strokes of its colour lifted most of the way to full value. The
 // light: the picture paled under strokes of its colour deepened.
 var art=select(fx_srgb(fx_lin(own)*mix(.36,.7,smoothstep(.35,.95,luma))),mix(own,vec3(1.),.3),light);
 if lit {art=select(mix(c,c/max(peak,.004),.8),c*.4,light);}
 return vec4(mix(own,art,input.params[0].y),input.color.a);
}
]])

-- The characters' parameters: a font pixel `px` points (1: a glyph 5 by 7
-- points, as small type), laid over the picture at `strength` (1), none in
-- cells darker than `threshold` (0.22), on light paper or dark.
function M.ascii_params(px: number?, strength: number?, threshold: number?, light: boolean?): { any }
	return { { px or 1, strength or 1, threshold or 0.22, if light then 1 else 0 }, { 0, 0, 0, 0 }, { 0, 0, 0, 0 }, { 0, 0, 0, 0 } }
end

-- Printed in dots, as Zeron's halftone (`halftone_pixels`): a screen of
-- dots `cell` points apart, of the picture's colour round them on paper:
-- in the dark, dots of light (the colour at its full value) on black,
-- wider where the picture is brighter; in the light, dots of ink (the
-- colour at its deepest) on white, wider where it's darker. Each dot's
-- size is the light it must give (linear light), so the print, seen from
-- where one reads, is the picture's own tone. Dots grow as a press's do
-- (a cosine spot): round, touching at half, then paper showing through
-- in round holes, with no cell's edge anywhere. The screen is turned 45°,
-- as a press turns its black: the eye reads a grid least along the
-- diagonals. Laid over the picture at `strength`.
-- params[0]: the cell (points), the strength (0 to 1), unused, light (1)
-- or dark (0).
M.halftone = ui.layer_shader(COMMON .. [[
fn layer(input:LayerInput)->vec4<f32> {
 let pitch=max(3.,round(input.params[0].x*input.scale));
 let d=fx_px(input);
 // (The colour round this pixel, a cell wide: it changes smoothly, so no
 // cell shows as a block of its own colour.)
 let c=fx_lin(fx_mean(input,d-pitch*.5,vec2(pitch)).rgb);
 let light=input.params[0].w>.5;
 // The paper keeps a breath of the picture's colour (6%), not a dead
 // black or white.
 let f=.06;
 var paper:vec3<f32>; var ink:vec3<f32>; var area:f32;
 if light {
  // Ink and paper give the colour exactly, the ink the deepest that can
  // (its weakest channel none); the colour a little softened first (70%
  // of its chroma), as ink on paper is, so no ink is garish.
  let t=mix(vec3(dot(c,vec3(.2126,.7152,.0722))),c,.7);
  let m=min(min(t.r,t.g),t.b);
  paper=vec3(1.)-(vec3(1.)-t)*f;
  area=clamp(1.-m/max(1.-(1.-m)*f,.001),0.,1.);
  ink=clamp((t-(1.-area)*paper)/max(area,.001),vec3(0.),vec3(1.));
 } else {
  let m=max(max(c.r,c.g),c.b);
  paper=c*f;
  area=clamp(m*(1.-f)/max(1.-f*m,.001),0.,1.);
  ink=c/max(m,.0001);
 }
 // The spot, on the screen turned 45°: 1 at a dot's middle, -1 between.
 let u=vec2(d.x+d.y,d.y-d.x)*.70710678/pitch*6.2831853;
 let spot=(cos(u.x)+cos(u.y))*.5;
 // The level the spot passes over `area` of the plane (fitted to the
 // cosine spot's own spread: 1 at none, 0 at half, -1 at all).
 let b=min(area,1.-area);
 let g=1.-2.*b-2.27*b*(.5-b);
 let level=select(-g,g,area<=.5);
 // Its edge antialiased across one device pixel.
 let slope=3.14159265/pitch*length(vec2(sin(u.x),sin(u.y)));
 var cover=clamp((spot-level)/max(slope,.001)+.5,0.,1.);
 cover=select(cover,0.,area<.002);
 cover=select(cover,1.,area>.998);
 let print=fx_srgb(mix(paper,ink,cover));
 return vec4(mix(input.color.rgb,print,input.params[0].y),input.color.a);
}
]])

-- The halftone's parameters: cells `cell` points across (3), laid at
-- `strength` (0.85), on light paper or dark.
function M.halftone_params(cell: number?, strength: number?, light: boolean?): { any }
	return { { cell or 3, strength or 0.85, 0, if light then 1 else 0 }, { 0, 0, 0, 0 }, { 0, 0, 0, 0 }, { 0, 0, 0, 0 } }
end

-- A display's lines, as Zeron's scanlines (`scanline_pixels`): every
-- `pitch` points a line one device pixel tall, darkened (in the light,
-- lifted towards white) by `strength`, less where the picture is bright
-- (a lit line blooms, as a tube's does); the rows between brightened a
-- little in linear light to give back some of what the lines take, so the
-- picture keeps its light rather than dimming as a whole.
-- params[0]: the pitch (points), the strength (0 to 1), unused, light (1)
-- or dark (0).
M.scanlines = ui.layer_shader(COMMON .. [[
fn layer(input:LayerInput)->vec4<f32> {
 let pitch=max(2.,round(input.params[0].x*input.scale));
 let row=floor(input.p.y*input.scale);
 let line=(row%pitch)<1.;
 let k=input.params[0].y;
 let c=fx_lin(input.color.rgb);
 let light=input.params[0].w>.5;
 let luma=dot(c,vec3(.2126,.7152,.0722));
 if light {
  let lift=k*(1.-.4*(1.-luma));
  let back=1.-.5*lift/(pitch-1.);
  return vec4(fx_srgb(select(c*back,c+(vec3(1.)-c)*lift,line)),input.color.a);
 }
 let dim=k*(1.-.45*sqrt(luma));
 let back=1.+.5*dim/(pitch-1.);
 return vec4(fx_srgb(select(c*back,c*(1.-dim),line)),input.color.a);
}
]])

-- The lines' parameters: every `pitch` points (1.5: Zeron's three source
-- rows), at `strength` (0.48: Zeron's), on light or dark.
function M.scanlines_params(pitch: number?, strength: number?, light: boolean?): { any }
	return { { pitch or 1.5, strength or 0.48, 0, if light then 1 else 0 }, { 0, 0, 0, 0 }, { 0, 0, 0, 0 }, { 0, 0, 0, 0 } }
end

return M
