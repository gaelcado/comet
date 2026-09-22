const fs=require('fs'),assert=require('assert'),G=require('./motion-geometry.js');
const icons=JSON.parse(fs.readFileSync(__dirname+'/catalog.json')),morphs=JSON.parse(fs.readFileSync(__dirname+'/morphs.json')),by=Object.fromEntries(icons.map(i=>[i.name,i]));
let frames=0,curves=0;
assert.equal(new Set(icons.map(i=>i.name)).size,icons.length);
// Exported artwork must be identical to the gallery's standard and optical masters.
for(const i of icons)for(const [folder,key,width] of [['svg','paths',1.5],['svg-small','smallPaths',1.75]]){
 const xml=fs.readFileSync(`${__dirname}/${folder}/${i.name}.svg`,'utf8');
 assert(xml.includes(`stroke-width="${width}"`));
 assert.deepEqual([...xml.matchAll(/ d="([^"]+)"/g)].map(m=>m[1]),i[key].map(p=>p.d));
 for(const p of i[key])for(const c of G.parse(p.d).curves)for(let n=0;n<=20;n++){
  const point=G.pointAt(c,n/20),inset=p.stroke===0?0:width/2;
  assert(point.every(v=>v>=inset&&v<=24-inset),`${i.name}: stroke clips the viewbox`);
 }
}
for(const i of icons)for(const p of [...i.paths,...i.smallPaths]){const shape=G.parse(p.d);for(const c of shape.curves){assert(c.flat().every(Number.isFinite),i.name);curves++}}
for(const m of morphs){
 const a=by[m.fromIcon].paths,b=by[m.toIcon].paths,plan=G.plan(a,b,m);
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
  if(!['eyelid','copy','diff','rotation'].includes(m.mode))assert(rendered,'shared contour moved: '+m.title);
 }
}
// Both side rails stay inset throughout travel, including their round caps.
for(const title of ['Left sidebar','Right sidebar','Terminal panel']){
 const m=morphs.find(m=>m.title===title),plan=G.plan(by[m.fromIcon].paths,by[m.toIcon].paths,m);
 for(let n=0;n<=100;n++){
  const paths=G.frame(plan,n/100);
  assert.equal(paths[0].d,by[m.fromIcon].paths[0].d,'Panel frame must stay fixed');
  for(const c of G.parse(paths[1].d).curves)for(let j=0;j<=10;j++){
   const [x,y]=G.pointAt(c,j/10);
   assert(x>=5.5-1e-6&&x<=18.5+1e-6&&y>=6.75-1e-6&&y<=17.25+1e-6,`${title}: rail clearance`);
  }
 }
}
const MotionState=require('./motion-state.js');
for(const speed of [.5,1,2]){
 const state=new MotionState();state.retarget(1,0,300,speed);state.sample(70,300,speed);
 const value=state.value,velocity=state.velocity;
 state.retarget(0,70,300,speed);
 assert.equal(state.value,value,'Reversal must preserve position');
 assert.equal(state.velocity,velocity,'Reversal must preserve velocity');
 for(let t=71;t<2000;t+=7){const v=state.sample(t,300,speed);assert(v>=0&&v<=1)}
 assert.equal(state.value,0);assert.equal(state.active,false);
 state.retarget(1,2000,300,speed);state.snap(1);
 assert.equal(state.sample(2050,300,speed),1);assert.equal(state.active,false);
}
const single=new MotionState(),stepped=new MotionState();
single.retarget(1,0,400);stepped.retarget(1,0,400);
single.sample(150,400);for(let t=10;t<=150;t+=10)stepped.sample(t,400);
assert(Math.abs(single.value-stepped.value)<1e-12,'Frame-rate dependent position');
assert(Math.abs(single.velocity-stepped.velocity)<1e-12,'Frame-rate dependent velocity');
// De Casteljau subdivision must preserve the curve, not approximate it.
const c=[[0,0],[5,2],[6,8],[12,12]],parts=G.split(c);
const at=(c,t)=>{const p=(a,b)=>a.map((x,i)=>x+(b[i]-x)*t),a=p(c[0],c[1]),b=p(c[1],c[2]),z=p(c[2],c[3]);return p(p(a,b),p(b,z))};
for(let j=0;j<=20;j++){const t=j/20,a=at(c,t),b=t<.5?at(parts[0],t*2):at(parts[1],(t-.5)*2);assert(Math.hypot(a[0]-b[0],a[1]-b[1])<1e-10)}
console.log(JSON.stringify({icons:icons.length,morphs:morphs.length,parsedCurves:curves,checkedFrameContours:frames,endpointFidelity:true,sharedContoursStable:true,exactSubdivision:true,errors:[]},null,2));
