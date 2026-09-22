"""Study 02: authored refinements. Executed by build.py before inventory generation."""
def redraw(name,*ds,note):
 icons[name]['paths']=[path(d) if isinstance(d,str) else d for d in ds]
 icons[name]['note']=note
# Continuous cubic corner shoulders replace the soft but pinched quadratic boxes.
# 0.5523 describes a circular fillet; .72 pulls the shoulder into a gentle squircle.
def capsule(x,y,w,h,r):
 k=.72*r
 return f'M{x+r} {y} H{x+w-r} C{x+w-r+k} {y} {x+w} {y+r-k} {x+w} {y+r} V{y+h-r} C{x+w} {y+h-r+k} {x+w-r+k} {y+h} {x+w-r} {y+h} H{x+r} C{x+r-k} {y+h} {x} {y+h-r+k} {x} {y+h-r} V{y+r} C{x} {y+r-k} {x+r-k} {y} {x+r} {y} Z'
# Closed rails retain a small inset handle: direction remains readable in both states.
fr=capsule(3,4.5,18,15,3)
for side,op,cl in [('left',8.75,5.5),('right',15.25,18.5)]:
 redraw(f'panel-{side}-open',fr,f'M{op} 5.25 V18.75',note='Continuous corner frame; panel divider meets the frame cleanly.')
 redraw(f'panel-{side}-closed',fr,f'M{cl} 10 V14',note='A short inset rail preserves which side will reopen, without a doubled border.')
redraw('panel-bottom-open',fr,'M3.75 14.25 H20.25',note='Same frame and contour order as the side rails.')
redraw('panel-bottom-closed',fr,'M10 17.25 H14',note='Horizontal handle keeps the closed bottom panel distinguishable.')
redraw('split',fr,'M12 5.25 V18.75',note='Matched frame and centred rule.')
redraw('unified',fr,'M7.5 9 H16.5','M7.5 12 H16.5','M7.5 15 H13.5',note='Text rows identify unified content; no ambiguous empty frame.')
redraw('window-maximize',capsule(4.5,4.5,15,15,2.75),note='Optically matched caption family.')
redraw('window-restore',capsule(4,8,12,12,2.5),'M8 4 H17 C19.2 4 20 4.8 20 7 V16',note='Clear back-window offset without doubled corners.')
redraw('home','M3.5 10.25 L10.25 4.75 Q12 3.3 13.75 4.75 L20.5 10.25','M6 10 V17.5 C6 19.5 6.5 20 8.5 20 H15.5 C17.5 20 18 19.5 18 17.5 V10','M10 20 V14.5 Q10 14 10.5 14 H13.5 Q14 14 14 14.5 V20',note='Roof and walls are optically joined; doorway has eased shoulders.')
# Zeron's own workspace silhouette: a short 45-degree shoulder and a generous flat body.
f='M3.5 8 C3.5 5.9 4.4 5 6.5 5 H9 L11.5 7.5 H17.5 C19.6 7.5 20.5 8.4 20.5 10.5 V17 C20.5 19.1 19.6 20 17.5 20 H6.5 C4.4 20 3.5 19.1 3.5 17 Z'
redraw('folder',f,note='Clipped tab shoulder and quiet continuous corners anchor the file family.')
redraw('folder-open','M3.5 11 V8 C3.5 5.9 4.4 5 6.5 5 H9 L11.5 7.5 H17.5 C19.6 7.5 20.5 8.4 20.5 10.5 V11','M6.5 11 H19.5 Q21.5 11 21 13 L19.5 18 Q19 20 17 20 H6 Q4 20 4.5 18 L5.5 13 Q6 11 6.5 11 Z',note='The front actually opens forward; no extra line across a closed folder.')
redraw('repository',f,'M8.5 12.5 L6.5 14.5 L8.5 16.5','M15.5 12.5 L17.5 14.5 L15.5 16.5',note='Two legible code brackets replace three cramped internal strokes.')
redraw('folder-add',f,'M9 14 H15','M12 11 V17',note='Shared folder body, centred addition mark.')
project='M7 3.5 H14.25 Q15.5 3.5 16.4 4.4 L19.6 7.6 Q20.5 8.5 20.5 9.75 V17 C20.5 19.5 19.5 20.5 17 20.5 H7 C4.5 20.5 3.5 19.5 3.5 17 V7 C3.5 4.5 4.5 3.5 7 3.5 Z'
redraw('project',project,'M8 8.5 H14.5 L9 15.5 H16',note='A restrained Z route inside a clipped workspace tile replaces the accidental A.')
redraw('monitor',capsule(3,4,18,12.5,2.75),'M12 16.5 V20','M8 20 H16',note='Wider screen counter, consistent pedestal.')
redraw('laptop',capsule(5,4,14,11.5,2.25),'M5 15.5 L3 18 Q2.2 19 3.5 19.5 Q4 20 5 20 H19 Q20 20 20.5 19.5 Q21.8 19 21 18 L19 15.5',note='Tapered base avoids the old swollen lower silhouette.')
redraw('phone',capsule(7,2.75,10,18.5,2.75),'M10.5 18.25 H13.5',note='Continuous handset corners with a clear home indicator.')
redraw('server',capsule(4,3.75,16,7,2.5),capsule(4,13.25,16,7,2.5),'M7.5 7.25 H7.6','M7.5 16.75 H7.6','M12.5 7.25 H16.5','M12.5 16.75 H16.5',note='Matched drive modules and precise indicator spacing.')
redraw('drive','M7 4.5 H17 Q18 4.5 18.25 5.5 L20.5 14 V17.5 Q20.5 20 18 20 H6 Q3.5 20 3.5 17.5 V14 L5.75 5.5 Q6 4.5 7 4.5 Z','M4 14 H20','M15.5 17 H17',note='Taper softened at the top; indicator separated from the enclosure.')
redraw('globe',circle(12,12,8.5),'M12 3.5 C7.5 7.25 7.5 16.75 12 20.5 C16.5 16.75 16.5 7.25 12 3.5 Z','M3.5 12 H20.5',note='One equator replaces two cramped latitude lines; readable at 16px.')
redraw('browser',fr,'M3.75 9 H20.25','M7 6.75 H7.1',note='Chrome divider stays clear of the frame corners.')
redraw('chat','M7 4.5 H17 C19.75 4.5 21 5.75 21 8.5 V13 C21 15.75 19.75 17 17 17 H10 L5.5 20.5 V16.75 C3.75 16.25 3 15 3 13 V8.5 C3 5.75 4.25 4.5 7 4.5 Z','M7.5 9 H16.5','M7.5 12.5 H13.5',note='A short deliberate tail and uneven text lengths give the conversation family character.')
redraw('chats','M6.5 4 H14.5 C17 4 18 5 18 7.5 V11.5 C18 14 17 15 14.5 15 H9 L5 18 V14.75 Q3 14 3 11.5 V7.5 C3 5 4 4 6.5 4 Z','M21 9 V16 C21 18 20.5 18.75 18.5 19 V21 L14.5 19 H11',note='Secondary bubble no longer creates a long confusing zigzag tail.')
# A single rounded diamond pin head; terminal tips remain round.
pn='M9 4.5 H15 L14.5 9.5 L17.5 13 Q18 14 16.5 14 H7.5 Q6 14 6.5 13 L9.5 9.5 Z'
for name,fill in [('pin',0),('pin-active',1)]:redraw(name,path(pn,fill),'M12 14 V20.5',note='Narrower neck, softened shoulder, same silhouette in selected state.')
redraw('unpin','M9.5 4.5 H15 L14.5 9.5 L17.5 13 Q18 14 16.5 14 H15.5','M9.5 9.5 L6.5 13 Q6 14 7.5 14 H11','M12 16.5 V20.5','M3.75 3.75 L20.25 20.25',note='Occluded contours are cut away beneath the slash instead of forming a black knot.')
redraw('calendar',capsule(3.5,5,17,15,2.75),'M8 3 V7','M16 3 V7','M4.25 10 H19.75','M8 14 H10','M14 14 H16',note='Two generous date marks instead of tightly spaced dots.')
# Key action glyphs: less oversized arrows, generous clipped contours.
redraw('send','M12 19.5 V4.5','M6.25 10.25 L12 4.5 L17.75 10.25',note='Compact head, balanced stem; matches the composer action weight.')
redraw('send-now','M4.5 12 H19.5','M13.75 6.25 L19.5 12 L13.75 17.75',note='Same arrow geometry as send, turned exactly 90 degrees.')
redraw('stop',capsule(6,6,12,12,2.25),note='Soft square is designed as a stable end pose for run controls.')
redraw('play','M8 6.25 C8 5.2 8.75 4.75 9.65 5.3 L18.45 10.65 C19.55 11.3 19.55 12.7 18.45 13.35 L9.65 18.7 C8.75 19.25 8 18.8 8 17.75 Z',note='Optically shifted right with equal corner tension; no pinched triangle tip.')
redraw('paperclip','M9 12 L14 7 Q15.75 5.25 17.5 7 Q19.25 8.75 17.5 10.5 L10 18 Q7 21 4 18 Q1 15 4 12 L12 4 Q15.5 .5 19 4',note='Nested parallel runs and even turn radii; counters stay open at 16px.')
redraw('edit','M5 15.5 L15.25 5.25 Q17 3.5 18.75 5.25 Q20.5 7 18.75 8.75 L8.5 19 H4 Z','M13.5 7 L17 10.5',note='Equal barrel width with a clipped graphite tip.')
redraw('copy',capsule(8,8,12,13,2.75),'M15.5 3.5 H6.5 C4.3 3.5 3.5 4.3 3.5 6.5 V15.5',note='Two unmistakable sheets and an even 4.5-unit offset.')
redraw('check','M5.5 12.25 L9.75 16.5 L18.75 7.5',note='Balanced ascent and shorter entry stroke; coherent at small sizes.')
redraw('microphone',capsule(9,3,6,11.5,3),'M6 10.5 V12 C6 15.9 8.1 18 12 18 C15.9 18 18 15.9 18 12 V10.5','M12 18 V21',note='Clean capsule, open cradle, no doubled bottom curve.')
redraw('microphone-off','M9 6 Q9 3 12 3 Q15 3 15 6 V10','M9 10 V11.5 Q9 14.5 12 14.5','M6 10.5 V12 Q6 18 12 18 Q14 18 15.5 17','M18 10.5 V12 Q18 13.25 17.75 14','M12 18 V21','M3.75 3.75 L20.25 20.25',note='Slash occludes the body and cradle; mute stays legible instead of overprinting lines.')
redraw('waveform','M4 10 V14','M8 6.5 V17.5','M12 4 V20','M16 8 V16','M20 10.5 V13.5',note='Asymmetric signal with a calm tapered envelope.')
# Folded sheets: compact fold, square shoulder, spacious internal symbols.
sh='M7 3.5 H14.5 L20 9 V17.5 C20 20 19 21 16.5 21 H7.5 C5 21 4 20 4 17.5 V7 C4 4.5 5 3.5 7 3.5 Z'
fo='M14.5 3.5 V7.5 Q14.5 9 16 9 H20'
for name in ['document','document-add','file-code','file-style','file-data','file-markdown','file-image']:
 icons[name]['paths'][:2]=[path(sh),path(fo)];icons[name]['note']='Shared clipped sheet with a smaller fold and a generous symbol counter.'
redraw('file-style',sh,fo,'M8 16.75 L12 11.75 L16 16.75 Z','M9.25 17 H14.75',note='A clear ink/drop silhouette replaces the illegible scribble.')
redraw('file-data',sh,fo,'M9.5 12 H8 V17.5 H9.5','M14.5 12 H16 V17.5 H14.5',note='Open brackets convey structured data without tiny interior dots.')
redraw('file-markdown',sh,fo,'M7.5 17 V12.5 L10.5 15.5 L13.5 12.5 V17','M16.5 12.5 V17','M15 15.5 L16.5 17 L18 15.5',note='Separated Markdown M and down arrow with larger counters.')
redraw('file-image',sh,fo,'M7.5 17.5 L10.25 14 L12.5 16.25 L15 13 L17 17.5',note='A single landscape contour keeps the tiny file badge from becoming noisy.')
redraw('image',capsule(3,4,18,16,3),'M3.75 17.75 L9 12.5 L12.5 16 L16.25 11.75 L20.25 16.5',circle(8,8.5,1),note='Landscape meets the inside of the frame; consistent image corner geometry.')
redraw('image-error',capsule(3,4,18,16,3),'M3.75 17.75 L8.5 13 L11.5 16','M16 8 V12.5','M16 15.75 V15.85',note='Warning area is deliberately kept clear of the image silhouette.')
redraw('save','M6.5 3.5 H15.5 Q16.5 3.5 17.25 4.25 L19.75 6.75 Q20.5 7.5 20.5 8.5 V17.5 C20.5 19.75 19.75 20.5 17.5 20.5 H6.5 C4.25 20.5 3.5 19.75 3.5 17.5 V6.5 C3.5 4.25 4.25 3.5 6.5 3.5 Z','M8 3.5 V8.5 H15 V3.5','M8 20.5 V15 Q8 14 9 14 H15 Q16 14 16 15 V20.5',note='Open lower recess eliminates the doubled bottom border.')
redraw('trash','M4.5 7 H19.5','M6.5 7 L7.25 18 Q7.4 20 9.5 20 H14.5 Q16.6 20 16.75 18 L17.5 7','M9 7 V4 Q9 3.5 9.5 3.5 H14.5 Q15 3.5 15 4 V7','M10 11 V16.5','M14 11 V16.5',note='Narrow tapered bin and inset slats replace a dense rectangular stack.')
redraw('eye','M3 12 C5.5 8.25 8.5 6.5 12 6.5 C15.5 6.5 18.5 8.25 21 12 C18.5 15.75 15.5 17.5 12 17.5 C8.5 17.5 5.5 15.75 3 12 Z',circle(12,12,2.5),note='Eyelid is a smooth symmetric lens with generous pupil clearance.')
redraw('eye-closed','M3 12 C5.5 15.75 8.5 17.5 12 17.5 C15.5 17.5 18.5 15.75 21 12','M6 15.5 L4.75 18','M12 17.5 V20','M18 15.5 L19.25 18',note='Closed state retains the lower lid exactly; lashes grow outward.')
redraw('wrap','M4 6 H19.5','M4 11 H16.5 C19.5 11 21 12.25 21 15 C21 17.75 19.5 19 16.5 19 H12','M14.5 16.5 L12 19 L14.5 21.5','M4 16 H7.5',note='Return loop opens up; arrow stays clear of adjacent text rows.')
redraw('unwrap','M4 6 H19.5','M4 11 H19.5','M4 16 H19.5','M4 21 H12',note='Rows align with the wrapped state, ready for a controlled loop transition.')
# Agent / capability glyphs carry more of the Zeron signature.
redraw('agent','M7.5 7 H15.25 Q16.5 7 17.4 7.9 L19.1 9.6 Q20 10.5 20 11.75 V16 C20 18.75 18.75 20 16 20 H8 C5.25 20 4 18.75 4 16 V11 C4 8.25 5.25 7 7.5 7 Z','M12 7 V4.5 L14 2.5','M8.25 12 V13.5','M15.25 12 V13.5','M9 16.75 H14.5',note='Clipped robot shoulder, offset antenna and calm facial features form a recognizable Zeron agent.')
redraw('skills','M5 18 L14.5 8.5 L17.5 11.5 L8 21 Z','M12.5 10.5 L15.5 13.5','M6.5 2.5 C6.5 5.5 5.5 6.5 2.5 6.5 C5.5 6.5 6.5 7.5 6.5 10.5 C6.5 7.5 7.5 6.5 10.5 6.5 C7.5 6.5 6.5 5.5 6.5 2.5 Z','M18.5 2.5 V6.5','M16.5 4.5 H20.5',note='A carved four-point spark and clipped wand echo the project shoulder.')
redraw('bolt','M13.5 3 L5 12.5 Q4.5 13 5.25 13 H10.75 L10.25 20.5 Q10.25 21 10.75 20.5 L19 11.5 Q19.5 11 18.75 11 H13.25 Z',note='Slightly eased lightning turns; distinctive narrow central offset.')
redraw('terminal',fr,'M7 9 L10.25 12 L7 15','M13 15 H17',note='Stable shared frame with an optically centred prompt.')
redraw('configure','M14 4 Q18 2.5 20.5 6 L16.5 6 L14 9 L17 12 L21 11.5 Q20 16 15 15 L8 21 Q7 22 6 21 L3 18 Q2 17 3 16 L9 10 Q8 5.5 12 3.5 L11.5 7.5',note='Readable open wrench jaw replaces a self-overlapping path.')
redraw('debug',capsule(7.5,8,9,12,4.5),'M9 8 V6 Q9 3 12 3 Q15 3 15 6 V8','M4 8 L7.5 11','M3.5 14 H7.5','M4.5 20 L8 17.5','M20 8 L16.5 11','M20.5 14 H16.5','M19.5 20 L16 17.5','M12 12 V19.25',note='Slimmer bug body and evenly radiating legs reduce congestion.')
# Theme: four long cardinal rays, shorter diagonals, centre retained through the eclipse.
redraw('sun',circle(12,12,4.25),*[line((12+math.cos(a)*7.25,12+math.sin(a)*7.25),(12+math.cos(a)*(9.5 if j%2==0 else 8.5),12+math.sin(a)*(9.5 if j%2==0 else 8.5))) for j,a in enumerate([i*math.pi/4 for i in range(8)])],note='Four cardinal rays and four shorter diagonals give a deliberate optical rhythm.')
redraw('moon','M20.25 12.75 C19.85 17.25 16.5 20.25 12 20.25 C7.45 20.25 3.75 16.55 3.75 12 C3.75 7.5 6.75 4.15 11.25 3.75 C8.5 9.25 14.75 15.5 20.25 12.75 Z',note='Broader crescent belly and a softer tip improve the 16px silhouette.')
redraw('appearance-system',capsule(3,4,18,12.5,2.75),'M12 16.5 V20','M8 20 H16','M12 4.75 V15.75',note='The half-screen divider stays inside the screen edge.')
redraw('lock',capsule(5.5,10,13,10.5,2.75),'M8.5 10 V7 Q8.5 3.5 12 3.5 Q15.5 3.5 15.5 7 V10','M12 14.25 V16.5',note='Rounded lock body with a slim centred key slot.')
redraw('unlock',capsule(5.5,10,13,10.5,2.75),'M8.5 10 V7 Q8.5 3.5 12 3.5 Q15.5 3.5 15.5 7 V7.5','M12 14.25 V16.5',note='A deliberate 2.5-unit shackle opening is visible at small sizes.')
# Eyelets stay smaller than the branch spine spacing.
redraw('pull-request-draft',circle(6,5.5,2),circle(6,18.5,2),circle(18,18.5,2),'M6 7.5 V16.5','M18 12.5 V13','M18 8.5 V9','M18 4.5 V5',note='Draft spine uses three short round dashes; matches the ready-state terminal nodes.')
redraw('file-tree','M5 4.5 V15.5 Q5 18 7.5 18 H11','M5 8 H11',capsule(12,4.5,8,7,2),capsule(12,14.5,8,7,2),note='Even vertical rhythm and connected branches without touching node corners.')
# Filled dots should not also have a 1.5-unit outline: that nearly doubled their diameter.
for name in ['more','drag','wifi','wifi-off']:
 for p in icons[name]['paths']:
  if p['fill']:p['stroke']=0
# Simplified optical artwork for small UI use. Canvas remains 24 units by design.
for name in icons:icons[name]['smallPaths']=icons[name]['paths']
for name,remove in {'calendar':[4,5],'keyboard':[2,3,4,5,6,7],'agent':[4],'debug':[8],'file-markdown':[3,4],'file-style':[3],'globe':[]}.items():
 icons[name]['smallPaths']=[p for n,p in enumerate(icons[name]['paths']) if n not in remove]
 icons[name]['smallNote']='Simplified interior detail for 12–16px rendering; stroke adjusted to 1.75 units.'
# Keep dates visibly present in the compact calendar.
icons['calendar']['smallPaths']=icons['calendar']['paths'][:4]+[path('M8 14 H12')]
icons['keyboard']['smallPaths']=[path(capsule(2.5,5,19,14,3)),path('M7 9 H7.1'),path('M12 9 H12.1'),path('M17 9 H17.1'),path('M8 14.5 H16')]
# Explicit pair mappings: [source contour, destination contour]. None is a reveal/retire.
# Unrelated silhouettes are staged, not pulled through arbitrary correspondences.
staged={'System appearance','Window size','Copy feedback','Send control','Pause agent','Queue edit','Dictation','Recording control','Loading result','Retry result','Line wrapping','Folder disclosure','PR merge','PR draft','Unpin action','Mic mute'}
for m in morphs:
 m['mode']='staged' if m['title'] in staged else 'morph'
 if m['title']=='Diff layout':m['mode']='staged'
 if m['title']=='Hidden files':m['mode']='eyelid'
 if m['title']=='Run control':m['mode']='morph'
 m['designNote']= {'staged':'Retain shared contours. Retire unmatched strokes, then draw the new mark; never distort unrelated parts.','morph':'Interpolate matching Bézier segments; retain corners and exact endpoints.','eyelid':'Close the upper lid into the lower arc, hide the pupil, then reveal lashes.'}[m['mode']]
# Corrections from the rendered midpoint audit, not just endpoint inspection.
redraw('file-style',sh,fo,'M12 11.5 C11 13 8.5 14.75 8.5 16 Q8.5 18 12 18 Q15.5 18 15.5 16 C15.5 14.75 13 13 12 11.5 Z',note='A fluid ink drop with a broad rounded bowl replaces the ambiguous angular mark.')
icons['file-style']['smallPaths']=icons['file-style']['paths']
redraw('bell-off','M8 7 Q9.5 5 12 5 Q17 5 17 10 Q17 11 17.2 12','M7 10 Q7 14 5 16 H14','M10 20 H14','M12 3 V5','M3.75 3.75 L20.25 20.25',note='Interrupted bell contours give the diagonal mute stroke its own clear channel.')
redraw('wifi-off','M3 8 Q5.5 6 7.25 5.5','M10.5 4.6 Q16 4.25 21 8','M6.5 12 Q8 10.75 10 10.25','M13.5 10 Q16 10.5 17.5 12','M10 16 Q12 14.5 14 16',path(circle(12,20,.8),1),'M3.75 3.75 L20.25 20.25',note='Wireless arcs are interrupted under the slash, avoiding dense crossings.')
icons['wifi-off']['paths'][5]['stroke']=0
for n in ['bell-off','wifi-off']:icons[n]['smallPaths']=icons[n]['paths']
for m in morphs:
 if m['title'] in ['Sound','Archive action','Notifications','Connection']:m['mode']='staged'
 if m['title'] in ['Disclosure','Show more','Fold all','Sort direction']:m['mode']='rotation'
 if m['title']=='Expand pane':m['mode']='staged'
 m['designNote']={'staged':'Shared shapes stay fixed while unrelated strokes exchange visibility.','morph':'Matched Bézier geometry retains exact curves and endpoints.','rotation':'Rigid rotation preserves the chevron or arrow silhouette through its midpoint.','eyelid':'The upper lid closes into the lower arc; pupil retires and lashes appear.'}[m['mode']]
for name,coordinates,radius in [('drag',[(x,y) for x in [9,15] for y in [6,12,18]],1),('more',[(x,12) for x in [5.5,12,18.5]],1.1)]:
 icons[name]['paths']=[dict(d=circle(x,y,radius),fill=1,stroke=0) for x,y in coordinates]
 icons[name]['smallPaths']=[dict(d=circle(x,y,1.125),fill=1,stroke=0) for x,y in coordinates]
 icons[name]['note']='Solid dots with no extra outline; optical dots stay 1.5px wide at a 16px render size.'
