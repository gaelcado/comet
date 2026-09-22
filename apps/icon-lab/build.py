"""Compile the editable icon and motion sources. No dependencies, no network."""
from pathlib import Path
import argparse,json,subprocess,sys,xml.etree.ElementTree as ET
ROOT=Path(__file__).resolve().parent

def svg(paths,stroke=1.5):
    root=ET.Element('svg',{'xmlns':'http://www.w3.org/2000/svg','viewBox':'0 0 24 24','fill':'none','stroke':'currentColor','stroke-width':str(stroke),'stroke-linecap':'round','stroke-linejoin':'round'})
    for p in paths:
        attributes={'d':p['d']}
        if p.get('fill'): attributes['fill']='currentColor'
        if p.get('stroke')==0: attributes['stroke']='none'
        ET.SubElement(root,'path',attributes)
    ET.indent(root,space='  ')
    return ET.tostring(root,encoding='unicode')+'\n'

def build(refresh=False):
    if refresh: subprocess.run([sys.executable,str(ROOT/'scan_inventory.py')],check=True)
    icons=json.loads((ROOT/'glyphs.json').read_text())
    motions=json.loads((ROOT/'motions.json').read_text())
    inventory=json.loads((ROOT/'inventory.json').read_text())
    names={i['name'] for i in icons}
    assert len(names)==len(icons),'Duplicate icon names'
    for record in inventory['desktop']+inventory['ios']+inventory['branchAdditions']:
        assert not record['custom'] or record['custom'] in names,record
    for m in motions: assert m['fromIcon'] in names and m['toIcon'] in names,m
    for size in ['svg','svg-small']:(ROOT/size).mkdir(exist_ok=True)
    for i in icons:
        assert i['paths'] and i['smallPaths'],i['name']
        i['legacy']=[e['asset'] for e in inventory['desktop'] if e['custom']==i['name']]
        i['references']=sorted({r for e in inventory['desktop']+inventory['ios'] if e['custom']==i['name'] for r in e['references']})
        i['status']='Used' if i['references'] else 'State / extension'
        if any(e['custom']==i['name'] for e in inventory['branchAdditions']):i['status']='Contribution'
        for folder,paths,width in [('svg',i['paths'],1.5),('svg-small',i['smallPaths'],1.75)]:
            text=svg(paths,width);ET.fromstring(text);(ROOT/folder/(i['name']+'.svg')).write_text(text)
    payload={'icons':icons,'morphs':motions,'inventory':inventory,'baseline':json.loads((ROOT/'study-02.json').read_text()),'firstStudy':json.loads((ROOT/'study-01.json').read_text())}
    (ROOT/'catalog.json').write_text(json.dumps(icons,indent=2)+'\n')
    (ROOT/'morphs.json').write_text(json.dumps(motions,indent=2)+'\n')
    (ROOT/'data.js').write_text('window.ZERON_ICONS = '+json.dumps(payload)+';\n')
    (ROOT/'favicon.svg').write_text(svg(next(i for i in icons if i['name']=='project')['paths']).replace('currentColor','#98501e'))
    print(f'Built {len(icons)} glyphs × 2 optical sizes and {len(motions)} motion specifications.')

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--refresh-inventory',action='store_true',help='Rescan the application and available contribution branches')
    args=parser.parse_args();build(args.refresh_inventory)
