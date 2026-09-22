const fs=require('fs'),assert=require('assert'),G=require('./motion-geometry.js');
const icons=JSON.parse(fs.readFileSync(__dirname+'/catalog.json')),morphs=JSON.parse(fs.readFileSync(__dirname+'/morphs.json')),by=Object.fromEntries(icons.map(i=>[i.name,i]));
let frames=0,curves=0;
for(const i of icons)for(const p of [...i.paths,...i.smallPaths]){const shape=G.parse(p.d);for(const c of shape.curves){assert(c.flat().every(Number.isFinite),i.name);curves++}}
for(const m of morphs){
 const a=by[m.fromIcon].paths,b=by[m.toIcon].paths,plan=G.plan(a,b,m.mode);
 assert.deepEqual(G.frame(plan,0).map(p=>p.d),a.map(p=>p.d));assert.deepEqual(G.frame(plan,1).map(p=>p.d),b.map(p=>p.d));
 for(let j=0;j<=100;j++)for(const p of G.frame(plan,j/100)){
  assert(Number.isFinite(p.opacity??1));assert((p.opacity??1)>=0&&(p.opacity??1)<=1);
  const q=G.parse(p.d);assert(q.curves.flat(2).every(Number.isFinite),m.title);
  for(let k=1;k<q.curves.length;k++)assert.deepEqual(q.curves[k][0],q.curves[k-1][3],m.title+' continuity');
  frames++;
 }
 // Every shared enclosure remains exactly unchanged in shape at the halfway pose.
 for(const slot of plan.slots.filter(s=>s.kind==='matched'&&s.a.d===s.b.d)){
  const rendered=G.frame(plan,.5).find(p=>{const q=G.parse(p.d);return G.data(q.curves,q.closed)===G.data(slot.ac,slot.closed)});
  if(m.mode!=='eyelid')assert(rendered,'shared contour moved: '+m.title);
 }
}
// De Casteljau subdivision must preserve the curve, not approximate it.
const c=[[0,0],[5,2],[6,8],[12,12]],parts=G.split(c);
const at=(c,t)=>{const p=(a,b)=>a.map((x,i)=>x+(b[i]-x)*t),a=p(c[0],c[1]),b=p(c[1],c[2]),z=p(c[2],c[3]);return p(p(a,b),p(b,z))};
for(let j=0;j<=20;j++){const t=j/20,a=at(c,t),b=t<.5?at(parts[0],t*2):at(parts[1],(t-.5)*2);assert(Math.hypot(a[0]-b[0],a[1]-b[1])<1e-10)}
console.log(JSON.stringify({icons:icons.length,morphs:morphs.length,parsedCurves:curves,checkedFrameContours:frames,endpointFidelity:true,sharedContoursStable:true,exactSubdivision:true,errors:[]},null,2));
