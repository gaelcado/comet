/* Shared, deterministic geometry for the gallery, exporters and motion tests. */
(function(root){
'use strict';
const mix=(a,b,t)=>a+(b-a)*t;
const pt=(a,b,t)=>[mix(a[0],b[0],t),mix(a[1],b[1],t)];
const clamp=x=>Math.max(0,Math.min(1,x));
const smooth=x=>{x=clamp(x);return x*x*(3-2*x)};
const cache=new Map();
function parse(d){
 if(cache.has(d))return cache.get(d);
 const tokens=d.match(/[A-Za-z]|[-+]?(?:\d*\.\d+|\d+\.?\d*)(?:[eE][-+]?\d+)?/g);
 if(!tokens)throw Error('Empty SVG path');
 let j=0,cmd,pos=[0,0],start,curves=[],closed=false;
 const number=()=>{const n=Number(tokens[j++]);if(!Number.isFinite(n))throw Error('Invalid SVG coordinate: '+d);return n};
 const point=()=>[number(),number()];
 const linear=to=>{curves.push([pos,pt(pos,to,1/3),pt(pos,to,2/3),to]);pos=to};
 while(j<tokens.length){
  if(/[A-Za-z]/.test(tokens[j]))cmd=tokens[j++];
  if(cmd==='M'){if(start)throw Error('One contour per path is required');pos=point();start=pos;cmd='L'}
  else if(cmd==='L')linear(point());
  else if(cmd==='H')linear([number(),pos[1]]);
  else if(cmd==='V')linear([pos[0],number()]);
  else if(cmd==='C'){const c1=point(),c2=point(),to=point();curves.push([pos,c1,c2,to]);pos=to}
  else if(cmd==='Q'){const c=point(),to=point();curves.push([pos,pt(pos,c,2/3),pt(to,c,2/3),to]);pos=to}
  else if(cmd==='Z'){if(!start)throw Error('Close without a move');if(Math.hypot(pos[0]-start[0],pos[1]-start[1])>1e-7)linear(start);closed=true;cmd=null}
  else throw Error('Unsupported SVG command: '+cmd);
 }
 if(!curves.length)throw Error('Empty contour '+d);
 const result={curves,closed};if(cache.size>=2048)cache.delete(cache.keys().next().value);cache.set(d,result);return result;
}
function split(c,t=.5){const a=pt(c[0],c[1],t),b=pt(c[1],c[2],t),z=pt(c[2],c[3],t),d=pt(a,b,t),e=pt(b,z,t),f=pt(d,e,t);return [[c[0],a,d,f],[f,e,z,c[3]]]}
function pointAt(c,t){const u=1-t;return [0,1].map(i=>u*u*u*c[0][i]+3*u*u*t*c[1][i]+3*u*t*t*c[2][i]+t*t*t*c[3][i])}
const length=c=>{let s=0,last=c[0];for(let i=1;i<=20;i++){const p=pointAt(c,i/20);s+=Math.hypot(p[0]-last[0],p[1]-last[1]);last=p}return s};
function equalize(cs,n){cs=cs.slice();while(cs.length<n){const k=cs.reduce((best,c,i)=>length(c)>length(cs[best])?i:best,0);cs.splice(k,1,...split(cs[k]))}return cs}
function cost(a,b){return a.reduce((s,c,i)=>s+c.reduce((r,p,j)=>r+(p[0]-b[i][j][0])**2+(p[1]-b[i][j][1])**2,0),0)}
const reverse=cs=>cs.slice().reverse().map(c=>c.slice().reverse());
function match(a,b,preserveDirection=false){
 const aa=parse(a.d),bb=parse(b.d),n=Math.max(aa.curves.length,bb.curves.length);
 const ac=equalize(aa.curves,n),bc=equalize(bb.curves,n);let best=bc,bestCost=cost(ac,bc);
 for(const candidate of preserveDirection?[bc]:[bc,reverse(bc)])for(let k=0;k<(aa.closed&&bb.closed?n:1);k++){
  const v=[...candidate.slice(k),...candidate.slice(0,k)],q=cost(ac,v);if(q<bestCost){best=v;bestCost=q}
 }
 return {a,b,ac,bc:best,closed:aa.closed&&bb.closed};
}
function data(cs,closed=false){return `M${cs[0][0].map(v=>+v.toFixed(5)).join(' ')} `+cs.map(c=>'C'+c.slice(1).flat().map(v=>+v.toFixed(5)).join(' ')).join(' ')+(closed?' Z':'')}
function interpolate(s,t){
 if(s.a.d===s.b.d)return {...s.a,fill:mix(s.a.fill||0,s.b.fill||0,t),opacity:1};
 return {d:data(s.ac.map((c,i)=>c.map((p,j)=>pt(p,s.bc[i][j],t))),s.closed),fill:mix(s.a.fill||0,s.b.fill||0,t),opacity:1,stroke:s.a.stroke===0&&s.b.stroke===0?0:1};
}
function transform(p,fn){const q=parse(p.d);return {...p,d:data(q.curves.map(c=>c.map(fn)),q.closed)}}
function rotate(p,degrees,cx,cy,scale=1){const a=degrees*Math.PI/180,co=Math.cos(a),si=Math.sin(a);return transform(p,([x,y])=>[cx+scale*((x-cx)*co-(y-cy)*si),cy+scale*((x-cx)*si+(y-cy)*co)])}
function scalePath(p,scale,opacity=1){const q=parse(p.d),coords=q.curves.flat(),center=[coords.reduce((a,p)=>a+p[0],0)/coords.length,coords.reduce((a,p)=>a+p[1],0)/coords.length];return {...transform(p,v=>pt(center,v,scale)),opacity}}
const metrics=new Map();
function trim(p,fraction){
 if(fraction>=1)return {...p,opacity:1};
 if(fraction<=0)return {...p,opacity:0};
 if(p.fill)return scalePath(p,.75+.25*fraction,fraction);
 const q=parse(p.d);
 if(!metrics.has(p.d))metrics.set(p.d,q.curves.map(length));
 const lengths=metrics.get(p.d),total=lengths.reduce((a,b)=>a+b,0);let remaining=fraction*total,kept=[];
 for(let i=0;i<q.curves.length;i++){
  if(remaining>=lengths[i]){kept.push(q.curves[i]);remaining-=lengths[i]}
  else{
   // Binary inversion of arc length keeps speed stable through curves and corners.
   let lo=0,hi=1;for(let j=0;j<14;j++){const u=(lo+hi)/2;if(length(split(q.curves[i],u)[0])<remaining)lo=u;else hi=u}
   kept.push(split(q.curves[i],(lo+hi)/2)[0]);break;
  }
 }
 return {...p,d:data(kept,false),opacity:clamp(fraction*4)};
}
function plan(a,b,spec={}){
 if(typeof spec==='string')spec={mode:spec};
 const mode=spec.mode||'morph',slots=[],usedA=new Set(),usedB=new Set();
 const pair=(i,j)=>{if(usedA.has(i)||usedB.has(j))return;if(!a[i]||!b[j])throw Error('Invalid contour pairing');slots.push({kind:'matched',...match(a[i],b[j]),i,j});usedA.add(i);usedB.add(j)};
 // Declared pairings are authoritative; then retain all remaining identical contours.
 for(const [i,j] of spec.pairs||[])pair(i,j);
 a.forEach((p,i)=>{if(!usedA.has(i)){const j=b.findIndex((q,j)=>!usedB.has(j)&&q.d===p.d);if(j>=0)pair(i,j)}});
 a.forEach((p,i)=>{if(!usedA.has(i))slots.push({kind:'out',p})});
 b.forEach((p,j)=>{if(!usedB.has(j))slots.push({kind:'in',p})});
 const result={a,b,mode,slots,spec};
 if(mode==='copy'){
  result.left=match({d:'M8 10.75 V18.25 C8 20.23 8.77 21 10.75 21',fill:0},{d:'M5.5 12.25 L9.75 16.5',fill:0},true);
  result.right=match({d:'M10.75 21 H17.25 C19.23 21 20 20.23 20 18.25 V10.75',fill:0},{d:'M9.75 16.5 L18.75 7.5',fill:0},true);
  result.top={d:'M8 10.75 C8 8.77 8.77 8 10.75 8 H17.25 C19.23 8 20 8.77 20 10.75',fill:0};
 }
 return result;
}
function frame(plan,t){
 t=clamp(t);if(t===0)return plan.a.map(p=>({...p,opacity:1}));if(t===1)return plan.b.map(p=>({...p,opacity:1}));
 const {a,b,mode,spec}=plan;
 if(mode==='rotation')return a.map((p,i)=>{const r=(spec.rotation||[]).find(r=>r.indices.includes(i));return r?{...rotate(p,r.angle*t,...r.center),opacity:1}:{...p,opacity:1}});
 if(mode==='copy')return [interpolate(plan.left,t),interpolate(plan.right,t),trim(plan.top,1-smooth(t/.7)),trim(a[1],1-smooth(t/.6))];
 if(mode==='diff')return [a[0],{...rotate(a[2],90*t,12,12,1-t/9),opacity:1},trim(a[1],1-smooth(t/.8)),trim(a[3],1-smooth(t/.8))];
 if(mode==='eyelid'){
  const top=`M3 12 C5.5 ${mix(8.25,15.75,t)} 8.5 ${mix(6.5,17.5,t)} 12 ${mix(6.5,17.5,t)} C15.5 ${mix(6.5,17.5,t)} 18.5 ${mix(8.25,15.75,t)} 21 12`;
  return [{d:top,opacity:1-smooth(t),fill:0},b[0],scalePath(a[1],1-.85*t,1-smooth(t/.65)),...b.slice(1).map(p=>trim(p,smooth((t-.35)/.65)))];
 }
 return plan.slots.map(s=>{
  if(s.kind==='matched')return interpolate(s,t);
  const visible=s.kind==='out'?1-smooth(t/.8):smooth((t-.15)/.85);
  return trim(s.p,visible);
 });
}
const api={parse,split,match,data,plan,frame,pointAt,trim,length};
if(typeof module!=='undefined')module.exports=api;else root.ZeronGeometry=api;
})(typeof window!=='undefined'?window:globalThis);
