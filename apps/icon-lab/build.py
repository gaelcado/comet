"""Original Zeron control geometry and reproducible usage inventory. No third-party paths."""
from pathlib import Path
import json,re,math,subprocess,collections,html
ROOT=Path(__file__).parent
REPO=ROOT.parent.parent
icons={}
def path(d,fill=0): return {'d':d,'fill':fill}
def line(*p): return 'M'+' L'.join(f'{x:g} {y:g}' for x,y in p)
def circle(x,y,r):
 k=r*.5522847498
 return f'M{x+r:g} {y:g} C{x+r:g} {y+k:g} {x+k:g} {y+r:g} {x:g} {y+r:g} C{x-k:g} {y+r:g} {x-r:g} {y+k:g} {x-r:g} {y:g} C{x-r:g} {y-k:g} {x-k:g} {y-r:g} {x:g} {y-r:g} C{x+k:g} {y-r:g} {x+r:g} {y-k:g} {x+r:g} {y:g} Z'
def box(x=3,y=4,w=18,h=16,r=3):
 return f'M{x+r} {y} H{x+w-r} Q{x+w} {y} {x+w} {y+r} V{y+h-r} Q{x+w} {y+h} {x+w-r} {y+h} H{x+r} Q{x} {y+h} {x} {y+h-r} V{y+r} Q{x} {y} {x+r} {y} Z'
def add(name,group,*ds,note=''):
 icons[name]={'name':name,'category':group,'paths':[path(d) if isinstance(d,str) else d for d in ds], 'note':note}
N='Navigation'; W='Workspace'; C='Composer'; F='Files'; G='Git & changes'; S='Settings'; A='Agents & tools'; T='Status'
# Navigation: identical frame starts and winding make panel morphs retain their silhouette.
frame=box(3,4.5,18,15,3)
for side,x in [('left',8.5),('right',15.5)]:
 for state,d in [('open',f'M{x} 5 V19'),('closed',f'M{3.8 if side=="left" else 20.2} 5 V19')]:
  add(f'panel-{side}-{state}',N,frame,d)
add('panel-bottom-open',N,frame,'M3.8 14.5 H20.2')
add('panel-bottom-closed',N,frame,'M3.8 18.7 H20.2')
for name,pts in {'left':[(18,12),(6,12),(11,7)],'right':[(6,12),(18,12),(13,7)],'up':[(12,18),(12,6),(7,11)],'down':[(12,6),(12,18),(7,13)]}.items():
 a,b,c=pts; d=(c[0],24-c[1]) if name in ['left','right'] else (24-c[0],c[1])
 add('arrow-'+name,N,line(a,b),line(c,b,d))
add('arrow-up-right',N,'M6.5 17.5 L17.5 6.5','M8 6.5 H17.5 V16')
add('return',N,'M19 6.5 V11 Q19 14 16 14 H5','M9 10 L5 14 L9 18')
for name,pts in {'down':[(7,10),(12,15),(17,10)],'up':[(7,14),(12,9),(17,14)],'left':[(14,7),(9,12),(14,17)],'right':[(10,7),(15,12),(10,17)]}.items(): add('chevron-'+name,N,line(*pts))
add('expand',N,'M9 4.5 H4.5 V9','M4.5 4.5 L9.5 9.5','M15 19.5 H19.5 V15','M19.5 19.5 L14.5 14.5')
add('collapse',N,'M4.5 9.5 H9.5 V4.5','M4.5 4.5 L9.5 9.5','M19.5 14.5 H14.5 V19.5','M19.5 19.5 L14.5 14.5')
add('fold',N,'M7 4 L12 9 L17 4','M7 20 L12 15 L17 20')
add('unfold',N,'M7 9 L12 4 L17 9','M7 15 L12 20 L17 15')
add('menu',N,'M5 7 H19','M5 12 H19','M5 17 H19')
add('more',N,*[path(circle(x,12,.9),1) for x in [5.5,12,18.5]])
add('home',N,'M3.5 10.5 L10.2 4.8 Q12 3.3 13.8 4.8 L20.5 10.5','M6 9 V18 Q6 20 8 20 H16 Q18 20 18 18 V9','M10 20 V14 H14 V20')
add('window-minimize',N,'M5 12 H19')
add('window-maximize',N,box(4.5,4.5,15,15,2.5))
add('window-restore',N,box(4.5,8,11.5,11.5,2.5),'M8 5 H17 Q20 5 20 8 V16')
# Workspace
folder='M3 8 Q3 5 6 5 H9 L11.5 7.5 H18 Q21 7.5 21 10.5 V17 Q21 20 18 20 H6 Q3 20 3 17 Z'
add('folder',W,folder)
add('folder-open',W,'M3 8 Q3 5 6 5 H9 L11.5 7.5 H18 Q21 7.5 21 10.5 V17 Q21 20 18 20 H6 Q3 20 3 17 Z','M3 12 H21')
add('repository',W,folder,'M8 13 L6.5 14.5 L8 16','M16 13 L17.5 14.5 L16 16','M13 12.5 L11 17')
add('folder-add',W,folder,'M9 14 H15','M12 11 V17')
add('project',W,box(4,4,16,16,4),'M8 15.5 L12 8.5 L16 15.5','M10 13 H14')
add('monitor',W,box(3,4,18,13,2.5),'M12 17 V20','M8 20 H16')
add('laptop',W,box(5,4,14,12,2),'M5 16 L2.5 19 Q2 20 4 20 H20 Q22 20 21.5 19 L19 16')
add('phone',W,box(7,2.5,10,19,2.5),'M10.5 18.5 H13.5')
add('server',W,box(4,3.5,16,7,2.5),box(4,13.5,16,7,2.5),'M8 7 H8.1','M8 17 H8.1','M13 7 H16','M13 17 H16')
add('drive',W,'M6 5 H18 L21 15 V18 Q21 20 19 20 H5 Q3 20 3 18 V15 Z','M3 15 H21','M16 17.5 H17.5')
add('cloud',W,'M7 18 H17 Q21 18 21 14 Q21 10 17 10 Q16 5 11 5 Q6 5 5.5 10 Q2.5 10.5 2.5 14 Q2.5 18 7 18 Z')
add('globe',W,circle(12,12,8.5),'M12 3.5 C7 7 7 17 12 20.5 C17 17 17 7 12 3.5 Z','M4 9 H20','M4 15 H20')
add('browser',W,frame,'M3.5 9 H20.5','M7 6.8 H7.1')
add('chat',W,'M7 4.5 H17 Q21 4.5 21 8.5 V14 Q21 18 17 18 H10 L5.5 21 V17.5 Q3 17 3 14 V8.5 Q3 4.5 7 4.5 Z','M7.5 9 H16.5','M7.5 13 H13.5')
add('chats',W,'M7 4 H16 Q19 4 19 7 V12 Q19 15 16 15 H9 L5 18 V14 Q3 13.5 3 11 V7 Q3 4 7 4 Z','M21 10 V17 Q21 19 19 19 V22 L15 19 H11')
add('archive',W,box(3,4,18,5,1.5),'M5 9 V18 Q5 20 7 20 H17 Q19 20 19 18 V9','M10 12.5 H14')
add('unarchive',W,box(3,4,18,5,1.5),'M5 9 V18 Q5 20 7 20 H17 Q19 20 19 18 V9','M12 18 V11.5','M9.5 14 L12 11.5 L14.5 14')
pin='M8 4.5 H16 L15 10 L18 14 H6 L9 10 Z'
add('pin',W,pin,'M12 14 V21')
add('pin-active',W,path(pin,1),'M12 14 V21')
add('unpin',W,pin,'M12 14 V21','M3.5 3.5 L20.5 20.5')
add('clock',W,circle(12,12,8.5),'M12 7 V12 L15.5 14')
add('calendar',W,box(3.5,5,17,15,3),'M8 3 V7','M16 3 V7','M4 10 H20','M8 14 H8.1','M12 14 H12.1','M16 14 H16.1')
# Composer
add('plus',C,'M5 12 H19','M12 5 V19')
add('close',C,'M6.5 6.5 L17.5 17.5','M17.5 6.5 L6.5 17.5')
add('send',C,'M12 20 V4','M5.5 10.5 L12 4 L18.5 10.5')
add('send-now',C,'M4 12 H20','M13.5 5.5 L20 12 L13.5 18.5')
add('stop',C,box(6,6,12,12,2))
add('play',C,'M8 5.5 Q8 4 9.5 5 L19 11 Q20.5 12 19 13 L9.5 19 Q8 20 8 18.5 Z')
add('pause',C,'M8 5 V19','M16 5 V19')
add('paperclip',C,'M8.5 12.5 L14 7 Q16 5 18 7 Q20 9 18 11 L10.5 18.5 Q7 22 3.8 18.5 Q.5 15.2 4 11.7 L12 3.7 Q15.5 .5 19 3.7')
add('edit',C,'M5 15.5 L15.5 5 Q17 3.5 18.5 5 L19 5.5 Q20.5 7 19 8.5 L8.5 19 H4 Z','M13.5 7 L17 10.5')
add('compose',C,'M10 4 H7 Q3.5 4 3.5 7.5 V17 Q3.5 20.5 7 20.5 H16.5 Q20 20.5 20 17 V14','M10 11.5 L17 4.5 Q18.5 3 20 4.5 Q21.5 6 20 7.5 L13 14.5 L9 15.5 Z')
add('copy',C,box(8,8,12,13,2.5),'M15.5 4 H6.5 Q3 4 3 7.5 V15.5')
add('check',T,'M5 12 L9.5 16.5 L19 7')
add('drag',C,*[path(circle(x,y,.7),1) for x in [9,15] for y in [6,12,18]])
add('microphone',C,box(9,3,6,12,3),'M6 10 V12 Q6 18 12 18 Q18 18 18 12 V10','M12 18 V21')
add('microphone-off',C,box(9,3,6,12,3),'M6 10 V12 Q6 18 12 18 Q18 18 18 12 V10','M12 18 V21','M3.5 3.5 L20.5 20.5')
add('waveform',C,'M4 10 V14','M8 6 V18','M12 3 V21','M16 7 V17','M20 10 V14')
add('link',C,'M10 7 L12.5 4.5 Q16 1 19.5 4.5 Q23 8 19.5 11.5 L17 14','M7 10 L4.5 12.5 Q1 16 4.5 19.5 Q8 23 11.5 19.5 L14 17','M8 16 L16 8')
# Files
sheet='M7 3 H14 L20 9 V18 Q20 21 17 21 H7 Q4 21 4 18 V6 Q4 3 7 3 Z'
fold='M14 3 V7 Q14 9 16 9 H20'
add('document',F,sheet,fold,'M8 13 H16','M8 16.5 H13')
add('document-add',F,sheet,fold,'M8.5 15 H15.5','M12 11.5 V18.5')
add('file-code',F,sheet,fold,'M9.5 12 L7 14.5 L9.5 17','M14.5 12 L17 14.5 L14.5 17')
add('file-style',F,sheet,fold,'M8 13 Q12 10 16 13 Q12 13 8 17 H16')
add('file-data',F,sheet,fold,'M9 12 H8 V17 H9','M15 12 H16 V17 H15','M12 13 V13.1','M12 16 V16.1')
add('file-markdown',F,sheet,fold,'M7.5 17 V12 L10 15 L12.5 12 V17','M16 12 V17','M14.5 15.5 L16 17 L17.5 15.5')
add('file-image',F,sheet,fold,'M7 18 L10 14 L12.5 16.5 L15 13 L18 18',circle(8.5,11,.7))
add('image',F,box(3,4,18,16,3),'M4 18 L9 12.5 L12.5 16 L16 11.5 L20.5 17',circle(8,8.5,1))
add('image-error',F,box(3,4,18,16,3),'M4 18 L9 12.5 L12 15.5','M16 8 V12','M16 15 V15.1')
add('save',F,'M6 3.5 H16 L20.5 8 V17.5 Q20.5 20.5 17.5 20.5 H6 Q3.5 20.5 3.5 17.5 V6.5 Q3.5 3.5 6 3.5 Z','M8 3.5 V9 H15 V3.5',box(8,14,8,6.5,1))
add('trash',F,'M4 7 H20','M6 7 L7 19 Q7 21 9 21 H15 Q17 21 17 19 L18 7','M9 7 V3.5 H15 V7','M10 11 V17','M14 11 V17')
add('eye',F,'M2.5 12 C7 4 17 4 21.5 12 C17 20 7 20 2.5 12 Z',circle(12,12,2.5))
add('eye-closed',F,'M2.5 12 C7 19 17 19 21.5 12','M6 16 L4.5 18.5','M12 17.5 V20','M18 16 L19.5 18.5')
add('search',F,circle(10.5,10.5,6.5),'M15.3 15.3 L20.5 20.5')
add('wrap',F,'M4 6 H20','M4 11 H16.5 Q21 11 21 15 Q21 19 16.5 19 H12','M15 16 L12 19 L15 22','M4 16 H7')
add('unwrap',F,'M4 6 H20','M4 11 H20','M4 16 H20','M4 21 H12')
add('split',G,frame,'M12 5 V19')
add('unified',G,frame,'M20.2 5 V19')
add('list',W,'M8 6 H20','M8 12 H20','M8 18 H20',*[path(circle(4,y,.65),1) for y in [6,12,18]])
add('grid',W,*[box(x,y,6.5,6.5,1.8) for x in [3.5,14] for y in [3.5,14]])
# Git
add('branch',G,circle(6,5.5,2),circle(6,18.5,2),circle(18,6.5,2),'M6 7.5 V16.5','M18 8.5 V10 Q18 13.5 12 13.5 Q6 13.5 6 16.5')
add('pull-request',G,circle(6,5.5,2),circle(6,18.5,2),circle(18,18.5,2),'M6 7.5 V16.5','M13 5.5 H15 Q18 5.5 18 8.5 V16.5','M15 3 L12.5 5.5 L15 8')
add('merge',G,circle(6,5.5,2),circle(18,18.5,2),circle(6,18.5,2),'M6 7.5 V16.5','M6 8 Q6 11 11 12 Q18 13 18 16.5')
add('commit',G,circle(12,12,3.5),'M3 12 H8.5','M15.5 12 H21')
add('tag',G,'M4 5.5 Q4 3.5 6 3.5 H12 L21 12.5 L12.5 21 L4 12.5 Z',circle(8,7.5,1))
add('sort',W,'M4 6 H20','M4 12 H15','M4 18 H10')
add('sort-ascending',W,'M5 6 H13','M5 12 H11','M5 18 H9','M18 19 V5','M15 8 L18 5 L21 8')
add('sort-descending',W,'M5 6 H13','M5 12 H11','M5 18 H9','M18 5 V19','M15 16 L18 19 L21 16')
# Settings & appearance. Moon starts at east and keeps clockwise circle correspondence.
add('sun',S,circle(12,12,4.2),*[line((12+math.cos(a)*7,12+math.sin(a)*7),(12+math.cos(a)*9.5,12+math.sin(a)*9.5)) for a in [i*math.pi/4 for i in range(8)]])
add('moon',S,'M20.5 12.5 C20.1 17.7 16.5 20.5 12 20.5 C7.3 20.5 3.5 16.7 3.5 12 C3.5 7.5 6.3 3.9 11.5 3.5 C8 9.5 14.5 16 20.5 12.5 Z')
add('appearance-system',S,box(3,4,18,13,2.5),'M12 17 V20','M8 20 H16','M12 4 V17')
add('settings',S,'M4 6 H10','M14 6 H20','M4 12 H6','M10 12 H20','M4 18 H14','M18 18 H20',circle(12,6,2),circle(8,12,2),circle(16,18,2))
add('tuning',S,'M6 4 V9','M6 13 V20','M12 4 V14','M12 18 V20','M18 4 V6','M18 10 V20',circle(6,11,2),circle(12,16,2),circle(18,8,2))
add('key',S,circle(8,8,4.5),'M11.5 11.5 L20 20','M16 16 L18.5 13.5','M18.5 18.5 L21 16')
add('lock',S,box(5,10,14,11,2.5),'M8 10 V7 Q8 3 12 3 Q16 3 16 7 V10','M12 14.5 V16.5')
add('unlock',S,box(5,10,14,11,2.5),'M8 10 V7 Q8 3 12 3 Q16 3 16 7 V7','M12 14.5 V16.5')
add('keyboard',S,box(2.5,5,19,14,3),*[f'M{x} {y} H{x+.1}' for y in [9,12] for x in [7,12,17]],'M8 15.5 H16')
add('command',S,'M8 8 H5.5 Q3 8 3 5.5 Q3 3 5.5 3 Q8 3 8 5.5 V18.5 Q8 21 5.5 21 Q3 21 3 18.5 Q3 16 5.5 16 H18.5 Q21 16 21 18.5 Q21 21 18.5 21 Q16 21 16 18.5 V5.5 Q16 3 18.5 3 Q21 3 21 5.5 Q21 8 18.5 8 Z')
add('logout',S,'M10 4 H7 Q4 4 4 7 V17 Q4 20 7 20 H10','M10 12 H21','M17 8 L21 12 L17 16')
add('user',S,circle(12,12,9),circle(12,9,3),'M5.5 18 Q7 14 12 14 Q17 14 18.5 18')
star='M12 3 L14.8 8.7 L21 9.6 L16.5 14 L17.6 20.2 L12 17.3 L6.4 20.2 L7.5 14 L3 9.6 L9.2 8.7 Z'
add('star',S,star)
add('star-active',S,path(star,1))
bell='M5 16 Q7 14 7 10 Q7 5 12 5 Q17 5 17 10 Q17 14 19 16 Z'
add('bell',S,bell,'M10 20 H14','M12 3 V5')
add('bell-off',S,bell,'M10 20 H14','M12 3 V5','M3.5 3.5 L20.5 20.5')
speaker='M3.5 9 H7 L12 5 V19 L7 15 H3.5 Z'
add('volume',S,speaker,'M15 8 Q19 12 15 16','M18 5 Q25 12 18 19')
add('volume-off',S,speaker,'M16 9 L21 15','M21 9 L16 15')
# Agent actions
add('terminal',A,frame,'M7 9 L10 12 L7 15','M13 15 H17')
add('agent',A,box(4,7,16,13,4),'M12 3 V7','M2 12 V15','M22 12 V15','M8.5 12 V13','M15.5 12 V13','M9 17 H15')
add('skills',A,'M5 18 L15 8 L18 11 L8 21 Z','M13 10 L16 13','M6 3 V9','M3 6 H9','M19 2 V6','M17 4 H21')
add('bolt',A,'M13.5 2.5 L4.5 13 H11 L10.5 21.5 L19.5 11 H13 Z')
add('test',A,'M9 3.5 H15','M10 3.5 V10 L4.5 18 Q3 20.5 6 20.5 H18 Q21 20.5 19.5 18 L14 10 V3.5','M7.5 14 H16.5')
add('lint',A,'M8 4 H18 Q20 4 20 6 V10','M8 4 L5 7 V10','M5 7 H8 V4','M4 16 L7.5 19.5 L14 13','M16 17 H20')
add('configure',A,'M14 3.5 Q19 2 21 7 L16 7 L14 10 L17 13 L21 12 Q19.5 17 14 15.5 L7 22 L2 17 L9 10 Q7.5 5 12 3 L11 8 L14 10')
add('build',A,'M3.5 7.5 L12 3 L20.5 7.5 V17 L12 21 L3.5 17 Z','M3.5 7.5 L12 12 L20.5 7.5','M12 12 V21','M8 5 L16.5 9.5')
add('debug',A,box(7,7,10,13,5),'M9 7 V5 Q12 1 15 5 V7','M3 9 L7 11','M3 15 H7','M4 21 L8 18','M21 9 L17 11','M21 15 H17','M20 21 L16 18','M12 12 V19')
add('checklist',A,'M3 6 L4.5 7.5 L7 4.5','M10 6 H21','M3 13 L4.5 14.5 L7 11.5','M10 13 H21','M4 20 H6','M10 20 H21')
add('checkbox',A,box(4,4,16,16,3))
add('checkbox-checked',A,box(4,4,16,16,3),'M8 12 L11 15 L16.5 9')
add('refresh',T,'M20 10 Q18.5 3 11.5 3.5 Q5 4 3.5 10','M20 4.5 V10 H14.5','M4 14 Q5.5 21 12.5 20.5 Q19 20 20.5 14','M4 19.5 V14 H9.5')
add('restart',T,'M4 10 Q5 3.5 12 3.5 Q20.5 3.5 20.5 12 Q20.5 20.5 12 20.5 Q7 20.5 4.5 17','M4 4.5 V10 H9.5')
add('wifi',T,'M3 8 Q12 1 21 8','M6.5 12 Q12 7.5 17.5 12','M10 16 Q12 14.5 14 16',path(circle(12,20,.7),1))
add('wifi-off',T,'M3 8 Q12 1 21 8','M6.5 12 Q12 7.5 17.5 12','M10 16 Q12 14.5 14 16',path(circle(12,20,.7),1),'M3.5 3.5 L20.5 20.5')
for name,parts in [('info',['M12 10.5 V16.5','M12 7.5 V7.6']),('success',['M7 12 L10.5 15.5 L17 9']),('error',['M8.5 8.5 L15.5 15.5','M15.5 8.5 L8.5 15.5']),('add',['M7 12 H17','M12 7 V17'])]: add(name,T,circle(12,12,9),*parts)
add('warning',T,'M10.2 4.5 Q12 1.5 13.8 4.5 L21 17.5 Q22.5 20 19.5 20 H4.5 Q1.5 20 3 17.5 Z','M12 9 V13','M12 16.5 V16.6')
add('loading',T,'M20.5 12 C20.5 16.7 16.7 20.5 12 20.5 C7.3 20.5 3.5 16.7 3.5 12 C3.5 7.3 7.3 3.5 12 3.5')

add('file-tree',F,'M5 4 V15 Q5 18 8 18 H11','M5 8 H11',box(12,4,8,7,2),box(12,14,8,7,2))
add('pull-request-draft',G,circle(6,5.5,2),circle(6,18.5,2),circle(18,18.5,2),'M6 7.5 V16.5','M18 13 V13.1','M18 9 V9.1','M18 5 V5.1')

aliases={'more-horizontal':'more','project-default':'project','remote-server':'server','smartphone':'phone','pen-new-square':'compose','drag-handle':'drag','queue-drag-handle':'drag','queue-send':'send-now','queue-check':'check','queue-close':'close','queue-paperclip':'paperclip','clock-circle':'clock','folder-with-files':'repository','floppy-disk':'save','git-branch':'branch','sidebar-minimalistic':'panel-right-open','sidebar-minimalistic-left':'panel-left-open','key-minimalistic':'key','alt-arrow-down':'chevron-down','alt-arrow-up':'chevron-up','alt-arrow-left':'chevron-left','alt-arrow-right':'chevron-right','expand-arrows':'expand','collapse-arrows':'collapse','fold-vertical':'fold','split-columns':'split','wrap-text':'wrap','archive-up-minimalistic':'unarchive','add-circle':'add','pin':'pin','pen':'edit','archive-minimalistic':'archive','trash-bin-minimalistic':'trash','settings-minimalistic':'settings','logout-2':'logout','magnifer':'search','palette-search':'search','global':'globe','widget':'grid','close-circle':'error','info-circle':'info','danger-triangle':'warning','chat-round-line':'chat','bot':'agent','volume-loud':'volume','hard-drive':'drive','action-play':'play','action-test':'test','action-lint':'lint','action-configure':'configure','action-build':'build','action-debug':'debug','star-bold':'star-active','sort-vertical':'sort-ascending','fast-tier':'bolt','magic-stick-3':'skills'}
# Explicit semantic correspondence; a missing slot grows/shrinks at its own centre.
morphs=[]
def pair(title,a,b,context,ms=220):
 assert a in icons and b in icons
 morphs.append(dict(title=title,fromIcon=a,toIcon=b,context=context,duration=ms))
pair('Appearance','sun','moon','Light / dark appearance',300)
pair('System appearance','moon','appearance-system','Dark / follow system',260)
pair('Left sidebar','panel-left-open','panel-left-closed','Project rail visibility',200)
pair('Right sidebar','panel-right-open','panel-right-closed','Inspector and side chats',200)
pair('Terminal panel','panel-bottom-open','panel-bottom-closed','Terminal visibility',200)
pair('Expand pane','expand','collapse','Expanded / restored pane',220)
pair('Window size','window-maximize','window-restore','Maximize / restore window',220)
pair('Disclosure','chevron-right','chevron-down','Tree, archive shelf and tool details',150)
pair('Show more','chevron-down','chevron-up','Message and picker expansion',150)
pair('Fold all','fold','unfold','Diff folding',180)
pair('Diff layout','unified','split','Unified / split diff',220)
pair('Line wrapping','unwrap','wrap','Code fence and diff wrapping',220)
pair('Copy feedback','copy','check','Clipboard copy succeeds',240)
pair('Run control','play','stop','Project action starts',220)
pair('Send control','send','stop','Composer starts a response',220)
pair('Pause agent','pause','play','Pause / resume; proposed control',220)
pair('Queue edit','edit','close','Enter / cancel editing queued prompt',220)
pair('Favorite','star','star-active','Model favorite',180)
pair('Pin session','pin','pin-active','Pinned state',180)
pair('Unpin action','pin','unpin','Pin / unpin context-menu action',200)
pair('Hidden files','eye','eye-closed','Show / hide ignored files',220)
pair('Sound','volume','volume-off','Notification audio',220)
pair('Notifications','bell','bell-off','Enable / silence notifications',220)
pair('Connection','wifi','wifi-off','Connected / disconnected',220)
pair('Credentials','lock','unlock','Secret lock state; proposed control',220)
pair('Dictation','microphone','waveform','Listening; voice contributions',240)
pair('Mic mute','microphone','microphone-off','Mute / unmute; proposed control',220)
pair('Recording control','microphone','stop','Start / stop dictation',220)
pair('Checklist item','checkbox','checkbox-checked','Markdown task completion',180)
pair('Archive action','archive','unarchive','Archive / restore session',220)
pair('Folder disclosure','folder','folder-open','File tree open / closed',180)
pair('Sort direction','sort-ascending','sort-descending','Ascending / descending',180)
pair('Loading result','loading','success','Asynchronous work completes',240)
pair('Retry result','error','refresh','Error / retry action',220)
pair('PR merge','pull-request','merge','Open / merged pull request',260)

pair('PR draft','pull-request-draft','pull-request','Draft / ready for review',240)

exec((ROOT/'polish.py').read_text())

# Inventory preserves every registered asset and every source reference, including unused entries.
registry=re.findall(r'\(([A-Z][A-Z_0-9]+), "([\w-]+)"\)',(REPO/'crates/ui/src/icons.rs').read_text())
source_files=list((REPO/'crates/ui/src').rglob('*.rs'))
refs=collections.defaultdict(list)
for p in source_files:
 for n,l in enumerate(p.read_text().splitlines(),1):
  for c in re.findall(r'icons::([A-Z][A-Z_0-9]+)',l): refs[c].append(f'{p.relative_to(REPO)}:{n}')
entries=[]
for const,old in registry:
 brand=old.endswith('-mark') or old=='zeron-logo'
 target=None if brand else aliases.get(old,old)
 if target: assert target in icons,(const,target)
 entries.append(dict(constant=const,asset=old,custom=target,disposition='Preserve brand identity' if brand else 'Custom replacement',references=refs[const]))
# SF Symbols direct literals, conditional alternatives, and tool symbol dispatch.
sf=collections.defaultdict(list)
for p in (REPO/'apps/ios/Zeron').rglob('*.swift'):
 for n,l in enumerate(p.read_text().splitlines(),1):
  if any(s in l for s in ['systemName:', 'systemImage:', 'iconButton(', 'case "exec":','case "readFile", "applyPatch":','case "writeFile":','case "editFile":','case "search":','case "glob":','case "webFetch",','case "todo":','default: return "square.grid']):
   for symbol in re.findall(r'"([a-z][a-z0-9.]+)"',l):
    if symbol not in ['exec','search','glob','todo']: sf[symbol].append(f'{p.relative_to(REPO)}:{n}')
sfmap={'doc.on.doc':'copy','photo.badge.exclamationmark':'image-error','macwindow':'browser','plus':'plus','arrow.up':'send','arrow.right':'send-now','chevron.up':'chevron-up','chevron.down':'chevron-down','trash':'trash','ellipsis':'more','photo':'image','xmark':'close','arrow.down':'arrow-down','chevron.right':'chevron-right','exclamationmark.triangle':'warning','bubble.left.and.text.bubble.right':'chats','checkmark':'check','archivebox':'archive','folder':'folder','bubble.left.and.bubble.right':'chats','chevron.left':'chevron-left','desktopcomputer':'monitor','person.circle':'user','folder.badge.plus':'folder-add','pin.fill':'pin-active','pin.slash':'unpin','pin':'pin','arrow.up.bin':'unarchive','checkmark.square.fill':'checkbox-checked','square':'checkbox','terminal':'terminal','doc.text':'document','doc.badge.plus':'document-add','pencil':'edit','magnifyingglass':'search','globe':'globe','checklist':'checklist','square.grid.2x2':'grid'}
ios=[dict(symbol=k,custom=sfmap.get(k),references=v) for k,v in sorted(sf.items()) if k in sfmap]
# Declared active branches are inspected without changing their checkouts.
branch_additions=[]
for branch in ['codex/rich-composer','codex/native-interactions','codex/compact-picker','wip/pr-board','feat/macos-composer-dictation','feat/ios-composer-dictation','wip/onboarding','feat/settings-modal','sidebar-spacing']:
 result=subprocess.run(['git','show',f'{branch}:crates/ui/src/icons.rs'],cwd=REPO,capture_output=True,text=True)
 if result.returncode: continue
 for c,a in re.findall(r'\(([A-Z][A-Z_0-9]+), "([\w-]+)"\)',result.stdout):
  if (c,a) not in registry: branch_additions.append(dict(branch=branch,constant=c,asset=a,custom=aliases.get(a,a)))
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip()
inv={'source':'zeronsh/zeron','sha':sha,'desktop':entries,'ios':ios,'branchAdditions':branch_additions,'fileIdentities':[str(p.relative_to(REPO)) for p in sorted((REPO/'crates/ui/assets/file-icons').rglob('*.svg'))], 'boundary':'Product controls across desktop and iOS; provider, language, app and user-uploaded project identities retained. Website marketing artwork excluded. Literal and dispatch source scan; runtime-supplied image names require integration audit.'}
for name,ic in icons.items():
 ic['legacy']=[e['asset'] for e in entries if e['custom']==name]
 ic['references']=sorted(set(r for e in entries+ios if e['custom']==name for r in e['references']))
 ic['status']='Used' if ic['references'] else ('Registered' if ic['legacy'] else 'State / extension')
 if any(e['custom']==name for e in branch_additions):ic['status']='Contribution'
 svg='<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">\n'+''.join(f'  <path d="{p["d"]}"'+(' fill="currentColor"' if p['fill'] else '')+(' stroke="none"' if p.get('stroke')==0 else '')+'/>\n' for p in ic['paths'])+'</svg>\n'
 (ROOT/'svg'/f'{name}.svg').write_text(svg)
(ROOT/'catalog.json').write_text(json.dumps(list(icons.values()),indent=2))
(ROOT/'inventory.json').write_text(json.dumps(inv,indent=2))
(ROOT/'morphs.json').write_text(json.dumps(morphs,indent=2))
(ROOT/'data.js').write_text('window.ZERON_ICONS = '+json.dumps({'icons':list(icons.values()),'morphs':morphs,'inventory':inv,'baseline':json.loads((ROOT/'study-01.json').read_text())})+';\n')
md=f'# Zeron icon inventory\n\nSource: `{sha}`. {len(entries)} desktop entries, {len(ios)} iOS symbol names, {len(inv["fileIdentities"])} file-identity SVGs.\n\n{inv["boundary"]}\n\n## Desktop registry\n\n| Constant | Existing asset | Custom glyph | References |\n| --- | --- | --- | --- |\n'
for e in entries:md+=f'| {e["constant"]} | {e["asset"]} | {e["custom"] or "Retain identity"} | '+(', '.join(e['references']) or 'Registered; no qualified references found')+' |\n'
md+='\n## iOS symbols\n\n| SF Symbol | Custom glyph | References |\n| --- | --- | --- |\n'
for e in ios:md+=f'| {e["symbol"]} | {e["custom"]} | '+', '.join(e['references'])+' |\n'
md+='\n## Contribution additions\n\n'+ '\n'.join(f'- `{e["branch"]}`: `{e["asset"]}` → `{e["custom"]}`' for e in branch_additions)
md+='\n\n## State transitions\n\n| Interaction | From | To | Duration | Context |\n| --- | --- | --- | --- | --- |\n'
for m in morphs:md+=f'| {m["title"]} | {m["fromIcon"]} | {m["toIcon"]} | {m["duration"]}ms | {m["context"]} |\n'
(ROOT/'INVENTORY.md').write_text(md)
print(f'Generated {len(icons)} original SVGs, {len(morphs)} morphs; {len(entries)} desktop mappings, {len(ios)} iOS names. Branch additions: {branch_additions}')
