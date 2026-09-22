// Native SwiftUI Canvas data compiled from the same reviewed vector paths.
const fs=require('fs'),path=require('path'),G=require('./motion-geometry.js');
const root=__dirname,icons=JSON.parse(fs.readFileSync(root+'/catalog.json')).map(({name,paths,smallPaths})=>({name,paths,smallPaths})),by=Object.fromEntries(icons.map(i=>[i.name,i]));
const motions=JSON.parse(fs.readFileSync(root+'/motions.json'));
const aliases=JSON.parse(fs.readFileSync(root+'/ios-symbols.json'));
const frames=motions.map(m=>{const plan=G.plan(by[m.fromIcon].paths,by[m.toIcon].paths,m);return {from:m.fromIcon,to:m.toIcon,duration:m.duration,frames:Array.from({length:97},(_,j)=>G.frame(plan,j/96)),smallFrames:Array.from({length:97},(_,j)=>G.frame(G.plan(by[m.fromIcon].smallPaths,by[m.toIcon].smallPaths,m),j/96))}});
fs.writeFileSync(path.resolve(root,'../ios/Zeron/Theme/CustomIconLibrary.json'),JSON.stringify({icons,aliases,motions:frames})+'\n');
console.log(`Exported ${icons.length} iOS glyphs and ${frames.length} transitions.`);
