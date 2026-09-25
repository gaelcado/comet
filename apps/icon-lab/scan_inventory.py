"""Refresh the checked-in app-control usage inventory from this checkout."""
from pathlib import Path
import json,re,subprocess,collections
ROOT=Path(__file__).resolve().parent
REPO=ROOT.parent.parent
icons={i['name']:i for i in json.loads((ROOT/'glyphs.json').read_text())}
aliases={'more-horizontal': 'more', 'project-default': 'project', 'remote-server': 'server', 'smartphone': 'phone', 'pen-new-square': 'compose', 'drag-handle': 'drag', 'queue-drag-handle': 'drag', 'queue-send': 'send-now', 'queue-check': 'check', 'queue-close': 'close', 'queue-paperclip': 'paperclip', 'clock-circle': 'clock', 'folder-with-files': 'repository', 'floppy-disk': 'save', 'git-branch': 'branch', 'sidebar-minimalistic': 'panel-right-open', 'sidebar-minimalistic-left': 'panel-left-open', 'key-minimalistic': 'key', 'alt-arrow-down': 'chevron-down', 'alt-arrow-up': 'chevron-up', 'alt-arrow-left': 'chevron-left', 'alt-arrow-right': 'chevron-right', 'expand-arrows': 'expand', 'collapse-arrows': 'collapse', 'fold-vertical': 'fold', 'split-columns': 'split', 'wrap-text': 'wrap', 'archive-up-minimalistic': 'unarchive', 'add-circle': 'add', 'pin': 'pin', 'pen': 'edit', 'archive-minimalistic': 'archive', 'trash-bin-minimalistic': 'trash', 'settings-minimalistic': 'settings', 'logout-2': 'logout', 'magnifer': 'search', 'palette-search': 'search', 'global': 'globe', 'widget': 'grid', 'close-circle': 'error', 'info-circle': 'info', 'danger-triangle': 'warning', 'chat-round-line': 'chat', 'bot': 'agent', 'volume-loud': 'volume', 'hard-drive': 'drive', 'action-play': 'play', 'action-test': 'test', 'action-lint': 'lint', 'action-configure': 'configure', 'action-build': 'build', 'action-debug': 'debug', 'star-bold': 'star-active', 'sort-vertical': 'sort-ascending', 'fast-tier': 'bolt', 'magic-stick-3': 'skills'}
# Inventory preserves every registered asset and every source reference, including unused entries.
registry=re.findall(r'\(([A-Z][A-Z_0-9]+), "([\w-]+)"\)',(REPO/'crates/ui/src/icons.rs').read_text())
if not registry:
 registry=re.findall(r'pub const ([A-Z][A-Z_0-9]+): &str = "(?:custom-icons|icons)/([\w-]+)\.svg";', (REPO/'crates/ui/src/experimental_icons/generated.rs').read_text())
source_files=[p for p in (REPO/'crates/ui/src').rglob('*.rs') if p.name not in {'ui_workbench.rs','experimental_icons.rs'} and 'experimental_icons' not in p.parts]
refs=collections.defaultdict(list)
for p in source_files:
 for n,l in enumerate(p.read_text().splitlines(),1):
  for c in re.findall(r'icons::([A-Z][A-Z_0-9]+)',l): refs[c].append(f'{p.relative_to(REPO)}:{n}')
entries=[]
for const,old in registry:
 brand=old.endswith('-mark') or old=='zeron-logo'
 target=None if brand else aliases.get(old,old)
 if target: assert target in icons,(const,target)
 entries.append(dict(constant=const,asset=old,custom=target,disposition='Preserve brand identity' if brand else 'Proposed replacement',references=refs[const]))
# SF Symbols direct literals, conditional alternatives, and tool symbol dispatch.
sf=collections.defaultdict(list)
for p in (REPO/'apps/ios/Zeron').rglob('*.swift'):
 for n,l in enumerate(p.read_text().splitlines(),1):
  if any(s in l for s in ['ZeronIcon(', 'zeronIcon:', 'systemName:', 'systemImage:', 'iconButton(', 'case "exec":','case "readFile", "applyPatch":','case "writeFile":','case "editFile":','case "search":','case "glob":','case "webFetch",','case "todo":','default: return "square.grid']):
   for symbol in re.findall(r'"([a-z][a-z0-9.]+)"',l):
    if symbol not in ['exec','search','glob','todo']: sf[symbol].append(f'{p.relative_to(REPO)}:{n}')
sfmap=json.loads((ROOT/'ios-symbols.json').read_text())
ios=[dict(symbol=k,custom=sfmap.get(k),references=v) for k,v in sorted(sf.items()) if k in sfmap]
# Declared active branches are inspected without changing their checkouts.
branch_additions=[]
for branch in ['codex/rich-composer','codex/native-interactions','codex/compact-picker','wip/pr-board','feat/macos-composer-dictation','feat/ios-composer-dictation','wip/onboarding','feat/settings-modal','sidebar-spacing']:
 result=subprocess.run(['git','show',f'{branch}:crates/ui/src/icons.rs'],cwd=REPO,capture_output=True,text=True)
 if result.returncode: continue
 for c,a in re.findall(r'\(([A-Z][A-Z_0-9]+), "([\w-]+)"\)',result.stdout):
  if c not in {name for name,_ in registry}: branch_additions.append(dict(branch=branch,constant=c,asset=a,custom=aliases.get(a,a)))
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip()
dirty=bool(subprocess.check_output(['git','status','--porcelain','--','crates/ui/src','apps/ios'],cwd=REPO,text=True).strip())
inv={'source':'zeronsh/zeron','sha':sha,'dirty':dirty,'desktop':entries,'ios':ios,'branchAdditions':branch_additions,'fileIdentities':[str(p.relative_to(REPO)) for p in sorted((REPO/'crates/ui/assets/file-icons').rglob('*.svg'))], 'boundary':'Current desktop and iOS icon references mapped to proposed replacements. Production assets remain unchanged. Provider, app and user imagery remain outside the study. Literal and dispatch source scan; runtime-supplied image names require review.'}
(ROOT/'inventory.json').write_text(json.dumps(inv,indent=2)+'\n')
print(f'Scanned {len(entries)} desktop entries and {len(ios)} iOS symbols at {sha[:12]}')
