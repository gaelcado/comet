'use strict';
const {icons,morphs,inventory,baseline}=window.ZERON_ICONS;
const byName=new Map(icons.map(i=>[i.name,i]));
const $=s=>document.querySelector(s), $$=s=>[...document.querySelectorAll(s)];
const NS='http://www.w3.org/2000/svg';
const safe=s=>String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
function svg(name,size=24,original=false){
 const i=original?baseline.find(i=>i.name===name):byName.get(name); if(!i) throw Error('Unknown icon '+name);
 return `<svg xmlns="${NS}" viewBox="0 0 24 24" width="${size}" height="${size}" fill="none" stroke="currentColor" stroke-width="${!original&&size<=16?1.75:1.5}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" style="width:${size}px;height:${size}px">${(!original&&size<=16?i.smallPaths||i.paths:i.paths).map(p=>`<path d="${p.d}"${p.fill?' fill="currentColor"':''}${p.stroke===0?' stroke="none"':''}/>`).join('')}</svg>`;
}
function exportSVG(name){return svg(name).replace(/ aria-hidden="true"/,'').replace(/ style="[^"]*"/,'').replace('><path','>\n  <path').replaceAll('/><path','/>\n  <path').replace('</svg>','\n</svg>')+'\n';}
// A single point correspondence per contour, retained throughout the transition.
// Bézier segments are matched by shared shape, winding and starting corner.
// Contours never get joined to unrelated subpaths. Missing parts collapse locally.
const samplingSVG=document.createElementNS(NS,'svg');
samplingSVG.setAttribute('width','0');samplingSVG.setAttribute('height','0');
samplingSVG.style.cssText='position:absolute;overflow:hidden;pointer-events:none';
samplingSVG.setAttribute('aria-hidden','true');document.body.append(samplingSVG);
const samples=new Map();
function sample(p){
 if(samples.has(p.d)) return samples.get(p.d);
 const el=document.createElementNS(NS,'path');el.setAttribute('d',p.d);samplingSVG.append(el);
 const length=el.getTotalLength(); const points=Array.from({length:97},(_,i)=>{const q=el.getPointAtLength(length*i/96);return [q.x,q.y]});el.remove();samples.set(p.d,points);return points;
}
function collapsed(points){let x=0,y=0;points.forEach(p=>{x+=p[0];y+=p[1]});return points.map(()=>[x/points.length,y/points.length]);}
const media=matchMedia('(prefers-reduced-motion: reduce)');
let manualReduce=false,speed=1;
const reduced=()=>media.matches||manualReduce;
const controllers=[];
class Morph {
 constructor(host,m,callback=()=>{}){
  this.host=host;this.m=m;this.callback=callback;this.value=0;this.target=0;this.frame=0;
  this.a=byName.get(m.fromIcon).paths;this.b=byName.get(m.toIcon).paths;
  host.innerHTML=svg(m.fromIcon);this.svg=host.querySelector('svg');this.svg.removeAttribute('style');
  this.plan=ZeronGeometry.plan(this.a,this.b,m.mode);
  controllers.push(this);this.draw(0);
 }
 draw(t){
  this.value=t;
  const paths=ZeronGeometry.frame(this.plan,t);
  while(this.svg.children.length>paths.length)this.svg.lastChild.remove();
  while(this.svg.children.length<paths.length)this.svg.append(document.createElementNS(NS,'path'));
  paths.forEach((p,i)=>{
   const el=this.svg.children[i];el.setAttribute('d',p.d);
   el.setAttribute('fill','currentColor');el.setAttribute('fill-opacity',p.fill||0);
   el.setAttribute('opacity',p.opacity??1);el.setAttribute('stroke',p.stroke===0?'none':'currentColor');
  });
  this.callback(t,this.target);
 }
 set(t){cancelAnimationFrame(this.frame);this.target=t;this.draw(t);}
 go(t){
  cancelAnimationFrame(this.frame);this.target=t;
  if(reduced()||document.hidden){this.draw(t);return;}
  const from=this.value,begin=performance.now(),duration=this.m.duration/speed*Math.max(.25,Math.abs(t-from));
  const tick=now=>{
   const p=Math.min(1,(now-begin)/duration),ease=1-Math.pow(1-p,3);
   this.draw(from+(t-from)*ease);
   if(p<1)this.frame=requestAnimationFrame(tick);else this.draw(t);
  };
  this.frame=requestAnimationFrame(tick);
 }
 toggle(){this.go(this.target===1?0:1);}
}
function motionPreference(){
 document.body.classList.toggle('reduce-motion',reduced());$('#os-motion').textContent=media.matches?'OS reduced motion is active':'';
 if(reduced())controllers.forEach(c=>c.set(c.target));
}
media.addEventListener('change',motionPreference);
$('#reduce').addEventListener('change',e=>{manualReduce=e.target.checked;motionPreference()});
$('#speed').addEventListener('change',e=>{speed=Number(e.target.value)});
document.addEventListener('visibilitychange',()=>{if(document.hidden)controllers.forEach(c=>c.set(c.target))});
const heroControllers=[];
for(const title of ['Appearance','Left sidebar','Run control']){
 const m=morphs.find(m=>m.title===title),btn=document.createElement('button');btn.className='hero-cell';btn.setAttribute('aria-label',`Preview ${title.toLowerCase()} transition`);btn.setAttribute('aria-pressed','false');
 btn.innerHTML=`<div class="hero-icon"></div><span>${safe(title)}</span>`;$('#hero-morphs').append(btn);
 const ctrl=new Morph(btn.querySelector('.hero-icon'),m,(_,target)=>btn.setAttribute('aria-pressed',String(target===1)));
 btn.addEventListener('click',()=>ctrl.toggle());heroControllers.push(ctrl);
}
$('#hero-replay').addEventListener('click',()=>{const to=heroControllers[0].target===1?0:1;heroControllers.forEach(c=>c.go(to))});
$('#theme').innerHTML=svg('moon',18);$('#theme').addEventListener('click',()=>{
 const dark=document.body.classList.toggle('dark');$('#theme').innerHTML=svg(dark?'sun':'moon',18);$('#theme').setAttribute('aria-label',`Switch to ${dark?'light':'dark'} background`);
});
$('#search-icon').innerHTML=svg('search',17);
$('#library-count').textContent=icons.length;$('#motion-count').textContent=morphs.length;$('#icon-total').textContent=icons.length;$('#motion-total').textContent=morphs.length;
let category='All',size=24,query='';
const categories=['All','Navigation','Workspace','Composer','Files','Git & changes','Agents & tools','Settings','Status'];
for(const c of categories){const b=document.createElement('button');b.textContent=c;b.className=c==='All'?'active':'';b.setAttribute('aria-pressed',String(c==='All'));b.addEventListener('click',()=>{category=c;$$('#categories button').forEach(x=>{x.classList.toggle('active',x===b);x.setAttribute('aria-pressed',String(x===b))});renderLibrary()});$('#categories').append(b)}
function renderLibrary(){
 const visible=icons.filter(i=>(category==='All'||i.category===category)&&[i.name,i.category,...i.legacy,...i.references].join(' ').toLowerCase().includes(query));
 $('#icon-grid').innerHTML=visible.map(i=>`<button class="icon-card" data-icon="${i.name}" aria-label="Inspect ${i.name}">${svg(i.name,size)}<span>${i.name}</span></button>`).join('');
 $$('#icon-grid button').forEach(b=>b.addEventListener('click',()=>inspect(b.dataset.icon)));$('#no-results').hidden=visible.length>0;$('#icon-grid').hidden=visible.length===0;
}
$('#search').addEventListener('input',e=>{query=e.target.value.trim().toLowerCase();renderLibrary()});
$$('[data-size]').forEach(b=>{b.setAttribute('aria-pressed',String(b.classList.contains('selected')));b.addEventListener('click',()=>{size=Number(b.dataset.size);$$('[data-size]').forEach(x=>{x.classList.toggle('selected',x===b);x.setAttribute('aria-pressed',String(x===b))});renderLibrary()})});
$('#grid-check').addEventListener('change',e=>document.body.classList.toggle('show-grid',e.target.checked));
$$('[data-view]').forEach(b=>b.addEventListener('click',()=>{
 $$('[data-view]').forEach(x=>{x.classList.toggle('active',x===b);x.setAttribute('aria-pressed',String(x===b))});$$('.view').forEach(v=>v.hidden=v.id!==b.dataset.view);
 // A hidden gallery must not leave unnecessary animation frames running.
 controllers.filter(c=>c.host.closest('[hidden]')).forEach(c=>c.set(c.target));
}));
$$('[data-view]').forEach(b=>b.setAttribute('aria-pressed',String(b.classList.contains('active'))));
document.addEventListener('keydown',e=>{
 if(e.key==='/'&&!['INPUT','TEXTAREA','SELECT'].includes(document.activeElement.tagName)&&!$('#inspector').open){e.preventDefault();$('[data-view="library"]').click();$('#search').focus()}
});
const motionControllers=[];
for(const m of morphs){
 const btn=document.createElement('button');btn.className='motion-card';btn.setAttribute('aria-label',`${m.title}: ${m.fromIcon} to ${m.toIcon}`);btn.setAttribute('aria-pressed','false');
 btn.innerHTML=`<div class="endpoints">${svg(m.fromIcon,16)}<span>→</span>${svg(m.toIcon,16)}</div><span class="duration">${m.duration} ms</span><div class="morph-display"></div><h3>${safe(m.title)}</h3><p>${safe(m.context)}<span class="motion-method">${m.mode==='staged'?'Shared contours + stroke handoff':m.mode==='eyelid'?'Anatomical eyelid closure':m.mode==='rotation'?'Rigid rotation':'Matched Bézier morph'}</span></p><div class="card-meta"><span class="card-state">${safe(m.fromIcon)}</span><span>Click to morph ↗</span></div>`;
 $('#motion-grid').append(btn);
 const ctrl=new Morph(btn.querySelector('.morph-display'),m,(t,target)=>{
  btn.querySelector('.card-state').textContent=t===0?m.fromIcon:t===1?m.toIcon:`${Math.round(t*100)}% → ${target===1?m.toIcon:m.fromIcon}`;
  btn.setAttribute('aria-pressed',String(target===1));
 });btn.addEventListener('click',()=>ctrl.toggle());motionControllers.push(ctrl);
}
$('#play-all').addEventListener('click',()=>{const t=motionControllers[0].target===1?0:1;motionControllers.forEach(c=>c.go(t));$('#timeline').value=t*100;$('#timeline-value').textContent=t*100+'%'});
$('#timeline').addEventListener('input',e=>{const t=Number(e.target.value)/100;motionControllers.forEach(c=>c.set(t));$('#timeline-value').textContent=Math.round(t*100)+'%'});
$('#coverage-stats').innerHTML=[[inventory.desktop.length,'Desktop registry entries'],[inventory.ios.length,'iOS symbol names'],[inventory.desktop.filter(e=>!e.custom).length,'Retained brand marks'],[inventory.fileIdentities.length,'Retained file identities']].map(([n,t])=>`<div class="stat"><strong>${n}</strong><span>${t}</span></div>`).join('');
$('#coverage-table').innerHTML=inventory.desktop.map(e=>`<tr><td>${safe(e.asset)}</td><td>${e.custom?`<button class="map-glyph" data-icon="${e.custom}">${svg(e.custom,18)}${safe(e.custom)}</button>`:'<span class="retained">Retain identity</span>'}</td><td>${e.custom?(e.references.length?e.references.length+' source references':'Registered, no qualified references'):'Brand mark'}</td></tr>`).join('');
$('#ios-table').innerHTML=inventory.ios.map(e=>`<tr><td>${safe(e.symbol)}</td><td><button class="map-glyph" data-icon="${e.custom}">${svg(e.custom,18)}${safe(e.custom)}</button></td><td>${e.references.length}</td></tr>`).join('');
$$('#coverage [data-icon]').forEach(b=>b.addEventListener('click',()=>inspect(b.dataset.icon)));
$('#branch-map').innerHTML=inventory.branchAdditions.map(e=>`<p><code>${safe(e.branch)}</code> · ${safe(e.asset)} → <strong>${safe(e.custom)}</strong></p>`).join('');
$('#audit-sha').textContent=`Desktop source snapshot: ${inventory.sha}. Source scan includes qualified Rust references, SwiftUI literals and tool-symbol dispatch. Runtime-supplied image names need an integration audit. See inventory.json for complete paths.`;
let selected='sun';
function inspect(name){
 selected=name;const i=byName.get(name);
 $('#inspect-title').textContent=name;$('#inspect-category').textContent=i.category+' · '+i.status;
 $('#inspect-preview').innerHTML=`<div class="compare-specimen">${svg(name,72,true)}<span>Study 01</span></div><div class="compare-specimen">${svg(name,72)}<span>Study 02</span></div>`;
 $('#inspect-note').textContent=i.note||'Retained after the family audit; familiar control with balanced geometry.';
 $('#inspect-sizes').innerHTML=[12,16,20,24,32].map(s=>`<div class="size-sample"><i>${svg(name,s)}</i><span>${s}px</span></div>`).join('');
 $('#inspect-aliases').textContent=i.legacy.length?'Replaces: '+i.legacy.join(', '):'State partner or additional control for the custom family.';
 $('#inspect-refs').innerHTML=i.references.length?i.references.map(r=>`<div>${safe(r)}</div>`).join(''):'No mainline call site. Review state partners and contribution additions in the usage map.';
 $('#ref-count').textContent=`(${i.references.length})`;$('#svg-source').textContent=exportSVG(name);$('#download-svg').href=`svg/${name}.svg`;$('#download-svg').download=`zeron-${name}.svg`;$('#download-small').href=`svg-small/${name}.svg`;$('#download-small').download=`zeron-${name}-small.svg`;
 $('#copy-svg').textContent='Copy SVG';$('#inspector').showModal();
}
$('#close-dialog').addEventListener('click',()=>$('#inspector').close());
$('#inspector').addEventListener('click',e=>{if(e.target===$('#inspector')){const r=e.target.getBoundingClientRect();if(e.clientX<r.left||e.clientX>r.right||e.clientY<r.top||e.clientY>r.bottom)e.target.close()}});
let toastTimer;
function toast(s){$('#toast').textContent=s;$('#toast').classList.add('visible');clearTimeout(toastTimer);toastTimer=setTimeout(()=>$('#toast').classList.remove('visible'),2200)}
$('#copy-svg').addEventListener('click',async()=>{
 try{await navigator.clipboard.writeText(exportSVG(selected));$('#copy-svg').textContent='Copied';toast('SVG copied');}
 catch{const r=document.createRange();r.selectNodeContents($('#svg-source'));const sel=getSelection();sel.removeAllRanges();sel.addRange(r);$('#svg-source').closest('details').open=true;toast('Select the SVG source and copy with ⌘C / Ctrl+C');}
});
// Lightweight diagnostics are also visible as console-free text at ?verify=1.
function verify(){
 const errors=[];for(const i of icons)for(const p of i.paths){const pts=sample(p);if(pts.some(q=>q.some(n=>!Number.isFinite(n))))errors.push(i.name+': nonfinite geometry');if(pts.some(([x,y])=>x<-.5||x>24.5||y<-.5||y>24.5))errors.push(i.name+': outside viewbox');}
 for(const c of motionControllers){for(const t of [0,.1,.25,.5,.75,.9,1]){c.set(t);if(c.svg.innerHTML.includes('NaN'))errors.push(c.m.title+': invalid morph')}c.set(0)}
 for(const e of [...inventory.desktop,...inventory.ios,...inventory.branchAdditions])if(e.custom&&!byName.has(e.custom))errors.push('Unmapped '+e.custom);
 return {icons:icons.length,morphs:morphs.length,errors};
}
renderLibrary();motionPreference();
if(location.search.includes('verify=1')){const v=verify(),p=document.createElement('pre');p.id='verification';p.style.cssText='position:fixed;bottom:5px;left:5px;background:var(--panel);padding:12px;border:1px solid var(--line);z-index:20';p.textContent=JSON.stringify(v);document.body.append(p)}

const refined=icons.filter(i=>JSON.stringify(i.paths)!==JSON.stringify(baseline.find(b=>b.name===i.name)?.paths));
$('#revision-summary').textContent=`${refined.length} glyphs refined, with simpler optical variants, intentional occlusion and shared frame geometry. Click any comparison for design notes and exports.`;
$('#revision-grid').innerHTML=refined.map(i=>`<button class="revision-card" data-icon="${i.name}"><div class="revision-glyphs"><div>${svg(i.name,40,true)}<span>01</span></div><span class="revision-arrow">→</span><div>${svg(i.name,40)}<span>02</span></div><div class="small-specimen">${svg(i.name,16)}<span>16px</span></div></div><h3>${i.name}</h3><p>${safe(i.note||'Refined optical weight and more consistent spacing.')}</p></button>`).join('');
$$('#revision-grid [data-icon]').forEach(b=>b.addEventListener('click',()=>inspect(b.dataset.icon)));
// Direct review links keep each QA surface reproducible.
const params=new URLSearchParams(location.search);
if(['library','motion','revisions','coverage'].includes(params.get('view')))document.querySelector(`[data-view="${params.get('view')}"]`)?.click();
if(params.get('pose'))motionControllers.forEach(c=>c.set(Number(params.get('pose'))));
if(params.get('dark')==='1')$('#theme').click();
