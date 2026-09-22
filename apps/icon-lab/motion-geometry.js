/* Pure geometry shared by the gallery, contact sheets and regression checks. */
(function(root){
'use strict';
const mix=(a,b,t)=>a+(b-a)*t;
const pt=(a,b,t)=>[mix(a[0],b[0],t),mix(a[1],b[1],t)];
const clamp=x=>Math.max(0,Math.min(1,x));
function parse(d){
 const tokens=d.match(/[A-Za-z]|[-+]?(?:\d*\.\d+|\d+\.?\d*)(?:[eE][-+]?\d+)?/g);let j=0,cmd,pos=[0,0],start,curves=[],closed=false;
 const number=()=>Number(tokens[j++]);const point=()=>[number(),number()];
 const linear=to=>{curves.push([pos,pt(pos,to,1/3),pt(pos,to,2/3),to]);pos=to};
 while(j<tokens.length){
  if(/[A-Za-z]/.test(tokens[j]))cmd=tokens[j++];
  if(cmd==='M'){pos=point();start=pos;cmd='L'}
  else if(cmd==='L')linear(point());
  else if(cmd==='H')linear([number(),pos[1]]);
  else if(cmd==='V')linear([pos[0],number()]);
  else if(cmd==='C'){const c1=point(),c2=point(),to=point();curves.push([pos,c1,c2,to]);pos=to}
  else if(cmd==='Q'){const c=point(),to=point();curves.push([pos,pt(pos,c,2/3),pt(to,c,2/3),to]);pos=to}
  else if(cmd==='Z'){if(Math.hypot(pos[0]-start[0],pos[1]-start[1])>1e-7)linear(start);closed=true;cmd=null}
  else throw Error('Unsupported SVG command: '+cmd);
 }
 if(!curves.length)throw Error('Empty contour '+d);
 return {curves,closed};
}
function split(c){const a=pt(c[0],c[1],.5),b=pt(c[1],c[2],.5),z=pt(c[2],c[3],.5),d=pt(a,b,.5),e=pt(b,z,.5),f=pt(d,e,.5);return [[c[0],a,d,f],[f,e,z,c[3]]]}
const length=c=>c.slice(1).reduce((s,p,j)=>s+Math.hypot(p[0]-c[j][0],p[1]-c[j][1]),0);
function equalize(cs,n){cs=cs.slice();while(cs.length<n){const k=cs.reduce((best,c,i)=>length(c)>length(cs[best])?i:best,0);cs.splice(k,1,...split(cs[k]))}return cs}
function cost(a,b){return a.reduce((s,c,i)=>s+c.reduce((r,p,j)=>r+(p[0]-b[i][j][0])**2+(p[1]-b[i][j][1])**2,0),0)}
const reverse=cs=>cs.slice().reverse().map(c=>c.slice().reverse());
function match(a,b){
 const aa=parse(a.d),bb=parse(b.d),n=Math.max(aa.curves.length,bb.curves.length);
 let ac=equalize(aa.curves,n),bc=equalize(bb.curves,n),best=bc,bestCost=cost(ac,bc);
 // Compare contour winding and start points. Closed curves retain closure throughout.
 for(const candidate of [bc,reverse(bc)])for(let k=0;k<(aa.closed&&bb.closed?n:1);k++){
  const v=[...candidate.slice(k),...candidate.slice(0,k)],q=cost(ac,v);if(q<bestCost){best=v;bestCost=q}
 }
 return {a,b,ac,bc:best,closed:aa.closed&&bb.closed};
}
function data(cs,closed=false){return `M${cs[0][0].map(v=>+v.toFixed(4)).join(' ')} `+cs.map(c=>'C'+c.slice(1).flat().map(v=>+v.toFixed(4)).join(' ')).join(' ')+(closed?' Z':'')}
function interpolate(s,t){return {d:data(s.ac.map((c,i)=>c.map((p,j)=>pt(p,s.bc[i][j],t))),s.closed),fill:mix(s.a.fill||0,s.b.fill||0,t),opacity:1,stroke:s.a.stroke===0&&s.b.stroke===0?0:1}}
function scalePath(p,scale,opacity){const q=parse(p.d),coords=q.curves.flat();const center=[coords.reduce((a,p)=>a+p[0],0)/coords.length,coords.reduce((a,p)=>a+p[1],0)/coords.length];return {...p,d:data(q.curves.map(c=>c.map(v=>pt(center,v,scale))),q.closed),opacity}}
function plan(a,b,mode='morph'){
 const used=new Set(),slots=[];
 // Exact shared geometry gets priority over array index, keeping enclosures stationary.
 a.forEach((p,i)=>{const j=b.findIndex((q,j)=>!used.has(j)&&q.d===p.d);if(j>=0){slots.push({kind:'matched',...match(p,b[j]),i,j});used.add(j)} });
 const ai=a.map((_,i)=>i).filter(i=>!slots.some(s=>s.i===i)),bi=b.map((_,i)=>i).filter(i=>!used.has(i));
 if(mode==='morph')while(ai.length&&bi.length){const i=ai.shift(),j=bi.shift();slots.push({kind:'matched',...match(a[i],b[j]),i,j})}
 ai.forEach(i=>slots.push({kind:'out',p:a[i]}));bi.forEach(j=>slots.push({kind:'in',p:b[j]}));
 return {a,b,mode,slots};
}
function frame(plan,t){
 if(t<=0)return plan.a.map(p=>({...p,opacity:1}));if(t>=1)return plan.b.map(p=>({...p,opacity:1}));
 if(plan.mode==='rotation'){
  const rotate=(p,angle,cx,cy)=>{const q=parse(p.d),co=Math.cos(angle),si=Math.sin(angle);return {...p,d:data(q.curves.map(c=>c.map(([x,y])=>[cx+(x-cx)*co-(y-cy)*si,cy+(x-cx)*si+(y-cy)*co])),q.closed),opacity:1}};
  const first=plan.a[0].d;
  if(plan.a.length===1)return [rotate(plan.a[0],(first==='M10 7 L15 12 L10 17'?Math.PI/2:Math.PI)*t,12,12)];
  if(plan.a.length===2)return plan.a.map((p,i)=>rotate(p,Math.PI*t,12,i===0?6.5:17.5));
  return plan.a.map((p,i)=>i<3?p:rotate(p,Math.PI*t,18,12));
 }
 if(plan.mode==='eyelid'){
  // Retain the lower eyelid; bring the upper half down to it without folding the whole lens.
  const top=`M3 12 C5.5 ${mix(8.25,15.75,t)} 8.5 ${mix(6.5,17.5,t)} 12 ${mix(6.5,17.5,t)} C15.5 ${mix(6.5,17.5,t)} 18.5 ${mix(8.25,15.75,t)} 21 12`;
  return [{d:top,opacity:1-t*.85,fill:0},plan.b[0],scalePath(plan.a[1],1-.85*t,1-clamp(t*1.8)),...plan.b.slice(1).map(p=>scalePath(p,.85+.15*t,clamp((t-.45)/.55)))];
 }
 return plan.slots.map(s=>{
  if(s.kind==='matched')return interpolate(s,t);
  const v=s.kind==='out'?1-t:t;
  // Short retreat avoids stringy collapsed curves. Shared frames carry spatial continuity.
  return scalePath(s.p,.94+.06*v,v);
 });
}
const api={parse,split,match,data,plan,frame};
if(typeof module!=='undefined')module.exports=api;else root.ZeronGeometry=api;
})(typeof window!=='undefined'?window:globalThis);
