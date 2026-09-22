const fs=require('fs'),G=require('./motion-geometry.js'),P=__dirname;
const icons=JSON.parse(fs.readFileSync(P+'/catalog.json')),base=JSON.parse(fs.readFileSync(P+'/study-02.json')),motions=JSON.parse(fs.readFileSync(P+'/morphs.json'));const by=Object.fromEntries(icons.map(i=>[i.name,i]));
const esc=s=>s.replaceAll('&','&amp;').replaceAll('<','&lt;');
const start=(w,h,title)=>`<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}"><rect width="${w}" height="${h}" fill="#f8f7f4"/><text x="25" y="38" font-family="sans-serif" font-size="20" fill="#262623">${esc(title)}</text>`;
const text=(x,y,s,size=10)=>`<text x="${x}" y="${y}" font-family="sans-serif" font-size="${size}" fill="#666960">${esc(s)}</text>`;
const glyph=(paths,x,y,size=24,stroke=1.5)=>`<g transform="translate(${x} ${y}) scale(${size/24})" stroke="#262623" fill="none" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round">`+paths.map(p=>`<path d="${p.d}" fill="#262623" fill-opacity="${p.fill||0}" opacity="${p.opacity??1}"${p.stroke===0?' stroke="none"':''}/>`).join('')+'</g>';
for(let part=0;part<3;part++){
 let sheet=start(1250,1430,`Optical audit ${part+1} / 12, 16, 20, 24px · light and dark`);
 icons.slice(part*45,part*45+45).forEach((i,j)=>{
  const x=j%5*250,y=70+Math.floor(j/5)*150;
  sheet+=text(x+15,y+18,i.name,11)+`<rect x="${x}" y="${y+80}" width="250" height="65" fill="#242623"/>`;
  [12,16,20,24].forEach((size,k)=>{
   const paths=size<=16?i.smallPaths:i.paths,width=size<=16?1.75:1.5;
   sheet+=glyph(paths,x+25+k*55,y+42-size/2,size,width);
   sheet+=glyph(paths,x+25+k*55,y+112-size/2,size,width).replaceAll('#262623','#f5f4ef');
  });
 });fs.writeFileSync(P+`/optical-sheet-${part+1}.svg`,sheet+'</svg>');
}
let s=start(1296,1700,'Zeron · Study 03 / 135 original controls');icons.forEach((i,j)=>{const x=j%9*144,y=80+Math.floor(j/9)*108;s+=`<rect x="${x}" y="${y}" width="144" height="108" fill="none" stroke="#deded7"/>`+glyph(i.paths,x+60,y+24)+text(x+12,y+80,i.name)});fs.writeFileSync(P+'/contact-sheet.svg',s+'</svg>');
const featured=['settings','tuning','pull-request','pull-request-draft','branch','merge','panel-left-open','panel-bottom-open','project','agent','folder-open','repository','skills','panel-left-closed','panel-right-closed','file-style','file-data','configure','microphone-off','unpin','play','globe','drag','save','calendar','paperclip'];
s=start(1000,80+featured.length*95,'Study 02 → Study 03 / form and optical detail');featured.forEach((n,j)=>{const y=80+j*95;s+=text(25,y+40,n,12)+glyph(base.find(i=>i.name===n).paths,220,y+12,48)+text(310,y+42,'→',20)+glyph(by[n].paths,370,y+12,48)+glyph(by[n].smallPaths,475,y+32,16,1.75)+text(530,y+34,(by[n].note||'').slice(0,67),10)+text(530,y+51,(by[n].note||'').slice(67),10)});fs.writeFileSync(P+'/comparison-sheet.svg',s+'</svg>');
for(let part=0;part<3;part++){
 const ms=motions.slice(part*12,part*12+12);s=start(960,80+ms.length*100,`Motion audit ${part+1} / endpoint, 25%, 50%, 75%, endpoint`);
 ms.forEach((m,j)=>{const y=80+j*100,plan=G.plan(by[m.fromIcon].paths,by[m.toIcon].paths,m);s+=text(20,y+35,m.title,12)+text(20,y+55,m.mode,10);[0,.25,.5,.75,1].forEach((t,k)=>{s+=glyph(G.frame(plan,t),230+k*140,y+12,48)})});fs.writeFileSync(P+`/motion-sheet-${part+1}.svg`,s+'</svg>');
}
