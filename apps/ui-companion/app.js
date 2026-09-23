const files = ['styleguide.json', 'components.json', '../icon-lab/catalog.json', '../icon-lab/motions.json'];
const $ = (selector, root = document) => root.querySelector(selector);
const $$ = (selector, root = document) => [...root.querySelectorAll(selector)];
const escapeHTML = value => String(value).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const state = {
  guide: null, components: [], icons: [], motions: [], iconMap: new Map(),
  appearance: localStorage.getItem('zeron-companion-appearance') || 'dark',
  accent: localStorage.getItem('zeron-companion-accent') || 'zeron',
  page: 'overview', componentCategory: 'All', iconCategory: 'All', iconSize: 24,
  motionItems: [], toastTimer: null,
};

function source(path, label = path) {
  const [file, line] = path.split(':');
  return `<a class="source-link" href="../../${escapeHTML(file)}${line ? '#L' + Number(line) : ''}" target="_blank" rel="noopener" title="${escapeHTML(path)}">${escapeHTML(label)} ↗</a>`;
}
function glyph(name, size = 18, label = '') {
  const item = state.iconMap.get(name) || state.iconMap.get('grid');
  if (!item) return '';
  const paths = size <= 16 ? (item.smallPaths || item.paths) : item.paths;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" role="img" ${label ? `aria-label="${escapeHTML(label)}"` : 'aria-hidden="true"'}>${paths.map(p => `<path d="${escapeHTML(p.d)}" fill="currentColor" fill-opacity="${p.fill || 0}" opacity="${p.opacity ?? 1}"${p.stroke === 0 ? ' stroke="none"' : ''}/>`).join('')}</svg>`;
}
function pathsSVG(paths, size = 40) {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths.map(p => `<path d="${escapeHTML(p.d)}" fill="currentColor" fill-opacity="${p.fill || 0}" opacity="${p.opacity ?? 1}"${p.stroke === 0 ? ' stroke="none"' : ''}/>`).join('')}</svg>`;
}
function metric(group, name) { return state.guide.metrics[group].find(item => item.name === name)?.value; }
function variable(name) { return '--' + name.replaceAll('_', '-'); }
function applyTheme() {
  const appearance = state.appearance;
  const role = state.guide.appearance[appearance];
  const accent = state.guide.accentPresets[state.accent][appearance];
  document.body.dataset.appearance = appearance;
  for (const [name, token] of Object.entries(role)) document.documentElement.style.setProperty(variable(name), token.value);
  const a = {
    accent: accent.primary, accent_strong: accent.strong,
    accent_wash: `color-mix(in srgb, ${appearance === 'dark' ? accent.strong : accent.primary} ${appearance === 'dark' ? 45 : 10}%, transparent)`,
    selection: `color-mix(in srgb, ${accent.primary} ${appearance === 'dark' ? 35 : 24}%, transparent)`,
    caret: accent.primary, busy: accent.primary, code_text: accent.primary,
    code_wash: `color-mix(in srgb, ${accent.primary} ${appearance === 'dark' ? 12 : 10}%, transparent)`,
  };
  for (const [name, value] of Object.entries(a)) document.documentElement.style.setProperty(variable(name), value);
  for (const name of ['CONTROL_RADIUS', 'PANEL_RADIUS']) document.documentElement.style.setProperty(variable(name.toLowerCase()), `${metric('Theme', name)}px`);
  document.documentElement.style.setProperty('--composer-radius', `${metric('Composer','COMPOSER_RADIUS')}px`);
  $$('[data-theme]').forEach(button => button.setAttribute('aria-pressed', String(button.dataset.theme === appearance)));
  $('#accent-select').value = state.accent;
  document.querySelector('meta[name="theme-color"]').content = role.surface.value;
  localStorage.setItem('zeron-companion-appearance', appearance);
  localStorage.setItem('zeron-companion-accent', state.accent);
}
function showToast(message) {
  const toast = $('#toast'); toast.textContent = message; toast.classList.add('show');
  clearTimeout(state.toastTimer); state.toastTimer = setTimeout(() => toast.classList.remove('show'), 2400);
}
function navigate() {
  const [hash, subsection] = location.hash.slice(1).split('/');
  const page = ['overview', 'foundations', 'components', 'patterns', 'icons'].includes(hash) ? hash : 'overview';
  state.page = page;
  $$('.page').forEach(section => section.classList.toggle('active', section.id === `page-${page}`));
  $$('.nav-link').forEach(link => { link.classList.toggle('active', link.dataset.page === page); link.setAttribute('aria-current', link.dataset.page === page ? 'page' : 'false'); });
  $('#current-page').textContent = ({overview:'Overview', foundations:'Foundations', components:'Components', patterns:'Patterns', icons:'Icons & motion'})[page];
  if (page === 'foundations' && ['color', 'type', 'layout', 'timing'].includes(subsection)) {
    document.getElementById(subsection)?.scrollIntoView({behavior:'instant'});
  } else {
    window.scrollTo({top:0, behavior:'instant'});
  }
}
function renderOverview() {
  $('#revision').textContent = `SOURCE ${state.guide.sourceRevision}`;
  $('#overview-stats').innerHTML = [
    [Object.values(state.guide.groups).flat().length, 'semantic color roles'],
    [Object.values(state.guide.metrics).flat().length, 'layout & type metrics'],
    [state.components.length, 'component specimens'],
    [state.icons.length, 'custom glyphs'],
  ].map(([value, label]) => `<div><strong>${value}</strong><span>${label}</span></div>`).join('');
  $('#overview-icons').innerHTML = ['panel-left-open','settings','pull-request','send'].map(name => glyph(name,25)).join('');
  $$('[data-icon]').forEach(el => { el.innerHTML = glyph(el.dataset.icon, 18); });
}
function renderFoundations() {
  const guide = state.guide;
  const color = Object.entries(guide.groups).map(([group, roles]) => `<div class="guide-group"><h3>${escapeHTML(group)}</h3><div class="swatch-grid">${roles.map(name => {
    const token = guide.appearance[state.appearance][name];
    const value = document.documentElement.style.getPropertyValue(variable(name)).trim() || token.value;
    return `<article class="swatch"><div class="swatch-color" style="--sample:${escapeHTML(value)}"></div><div class="swatch-meta"><b>${escapeHTML(name)}</b><small>${escapeHTML(value)}</small>${source(token.source, token.expression)}</div></article>`;
  }).join('')}</div></div>`).join('');
  const typeRows = [[32,'Display','The shape of Zeron'],[24,'Title','A focused workspace'],[16,'Body','The message is the center of the interface.'],[13,'Row','Settings and compact controls'],[12,'Description','Secondary information stays readable.'],[10.5,'Badge','ACTIVE']];
  const metrics = Object.entries(guide.metrics).map(([group, items]) => `<div class="guide-group"><h3>${escapeHTML(group)}</h3><div class="metric-grid">${items.map(item => `<div class="metric-card"><strong>${item.value % 1 ? item.value : item.value.toFixed(0)}<span style="font-size:12px;color:var(--text-faint);margin-left:3px">px</span></strong><small>${escapeHTML(item.name.toLowerCase())}</small>${source(item.source, 'View source')}</div>`).join('')}</div></div>`).join('');
  const timings = guide.motion.map(item => `<div class="motion-spec"><b>${escapeHTML(item.name.replaceAll('_',' ').toLowerCase())}</b><strong>${item.duration}ms</strong><p>${escapeHTML(item.curve)}${item.bezier ? ` · ${item.bezier.join(', ')}` : ''}</p><div class="timing-line" style="--duration:${Math.max(7, item.duration / 500 * 100)}%"></div>${source(item.source, 'View source')}</div>`).join('');
  $('#foundation-content').innerHTML = `
    <section id="color" class="guide-section"><div class="guide-heading"><h2>Color</h2><p>Semantic roles from ${escapeHTML(guide.source)}. Swatches reflect the selected appearance and accent.</p></div>${color}</section>
    <section id="type" class="guide-section"><div class="guide-heading"><h2>Typography</h2><p>Bundled Geist for interface text and Geist Mono for code. Size examples follow app conventions; code and terminal defaults are extracted below.</p></div><div class="type-stage"><div class="type-card"><small>GEIST / INTERFACE</small><div class="type-sample">Shape and intent.</div><small>REGULAR · MEDIUM · SEMIBOLD</small></div><div class="type-card mono"><small>GEIST MONO / CODE</small><div class="type-sample">const idea = true;</div><small>FILE PATHS · CODE · KEYBOARD</small></div></div><div class="type-scale">${typeRows.map(([size, label, text]) => `<div class="type-row"><code>${size}px</code><strong style="font-size:${size}px">${escapeHTML(text)}</strong><small>${label}</small></div>`).join('')}</div><p class="source-link" style="margin-top:10px">Font source: ${source('crates/ui/src/typography.rs', 'typography.rs')}</p></section>
    <section id="layout" class="guide-section"><div class="guide-heading"><h2>Space & shape</h2><p>Numbers drive layout independently of theme. Each metric links to its Rust definition.</p></div><div class="spacing-visual">${['SPACE_XS','SPACE_SM','SPACE_MD','SPACE_LG'].map(name => `<div><i style="--step:${metric('Theme', name) * 2}px"></i>${metric('Theme', name)}px</div>`).join('')}</div>${metrics}</section>
    <section id="timing" class="guide-section"><div class="guide-heading"><h2>Motion timing</h2><p>Native durations and easing names. The icon motion preview below uses the exact custom geometry.</p></div><div class="timing-grid">${timings}</div></section>`;
}
function previewFor(item) {
  const i = (name, size=16) => glyph(name, size);
  const button = (name, kind='quiet', icon='') => `<button class="button ${kind}" data-toast="${escapeHTML(name)} selected">${icon ? i(icon) : ''}${escapeHTML(name)}</button>`;
  switch(item.id) {
    case 'button': return `<div class="demo-stack"><div class="demo-actions">${button('Continue','solid')}${button('Secondary')}${button('Ghost','ghost')}</div><div class="demo-actions">${button('Delete','danger')}<button class="button quiet" disabled>Disabled</button></div></div>`;
    case 'icon-button': return `<div class="demo-actions">${['panel-left-open','search','settings','more','close'].map(name => `<button class="demo-icon-button" aria-label="${name}" data-toast="${name}">${i(name)}</button>`).join('')}</div>`;
    case 'segmented': return `<div class="demo-actions"><div class="segmented" role="group" aria-label="Diff layout">${[['unified','Unified'],['split','Split']].map(([name,label],ix) => `<button aria-pressed="${ix === 0}" data-segment>${i(name,14)} ${label}</button>`).join('')}</div></div>`;
    case 'toggle': return `<div class="demo-actions" style="gap:25px"><label class="demo-inline">Off <button class="demo-switch" role="switch" aria-label="Example setting" aria-checked="false"></button></label><label class="demo-inline">On <button class="demo-switch" role="switch" aria-label="Another setting" aria-checked="true"></button></label></div>`;
    case 'checkbox': return `<div class="demo-actions"><label class="demo-inline"><input type="checkbox" class="demo-check">Show hidden files</label><label class="demo-inline"><input type="checkbox" class="demo-check" checked>Done</label></div>`;
    case 'field': return `<label class="field-label">Display name<input class="demo-field" placeholder="Your name"><small class="field-help">This name appears on your device.</small></label>`;
    case 'search': return `<label class="search-field" style="max-width:none;width:100%">${i('search')}<input type="search" placeholder="Search files and sessions…"><kbd style="color:var(--text-faint);font:9px 'Geist Mono'">⌘ F</kbd></label>`;
    case 'select': return `<div class="demo-stack"><select class="demo-select" aria-label="Choose an appearance"><option>Follow system</option><option>Dark</option><option>Light</option></select></div>`;
    case 'badge': return `<div class="demo-actions"><span class="badge">Default</span><span class="badge active">Active</span><span class="badge warn">Awaiting input</span><span class="badge danger">Failed</span></div>`;
    case 'status': return `<div class="demo-actions"><span><i class="status-dot busy"></i>Working</span><span><i class="status-dot"></i>Completed</span><span><i class="status-dot warn"></i>Warning</span><span><i class="status-dot error"></i>Error</span></div>`;
    case 'alert': return `<div class="demo-stack"><div class="demo-alert">${i('warning')} Could not reach this device. Check your connection.</div><div class="demo-alert warn">${i('info')} A new version is ready to install.</div></div>`;
    case 'toast': return `<div class="demo-actions">${button('Show confirmation','quiet','check')}</div>`;
    case 'menu': return `<div class="demo-menu">${[['compose','New session','⌘ N'],['settings','Settings','⌘ ,'],['logout','Sign out','']].map(([name,label,key]) => `<button data-toast="${label}">${i(name)}${label}<kbd>${key}</kbd></button>`).join('')}</div>`;
    case 'tabs': return `<div class="demo-tabs" role="tablist">${[['chat','Design review'],['terminal','Terminal'],['split','Changes']].map(([name,label],ix) => `<button role="tab" aria-selected="${ix === 0}" data-tab>${i(name,13)} ${label}</button>`).join('')}</div>`;
    case 'sidebar': return `<div class="demo-session-row">${[['Working on UI companion','busy'],['Refining editor',''],['Shipping polish','success']].map(([label,status],ix) => `<button style="${ix===0?'background:var(--element-active);color:var(--text)':''}"><i class="status-dot ${status}"></i>${label}</button>`).join('')}</div>`;
    case 'breadcrumb': return `<div class="demo-inline" style="color:var(--text-muted)">${i('folder')}src ${i('chevron-right',12)} ui ${i('chevron-right',12)} theme.rs</div>`;
    case 'settings-row': return `<div class="demo-settings-row"><div class="row-tile">${i('bell')}</div><div><strong>Completion notifications</strong><small>Play a sound when the agent finishes</small></div><button class="demo-switch" role="switch" aria-label="Completion notifications" aria-checked="true"></button></div>`;
    case 'file-row': return `<div class="demo-stack"><div class="demo-file-row">${i('file-code')} src/theme.rs <span class="mod">M</span></div><div class="demo-file-row">${i('file-style')} app.css <span class="mod">M</span></div></div>`;
    case 'queue': return `<div class="demo-settings-row">${i('drag')}<div><strong>Review the layout</strong><small>Queued · ready to send</small></div>${button('Send','quiet','send')}</div>`;
    case 'pull-request': return `<div class="demo-actions">${i('pull-request',22)}<span class="badge">Open</span>${i('merge',22)}<span class="badge active">Merged</span></div>`;
    case 'composer': return `<div class="demo-composer"><textarea placeholder="Ask Zeron anything…" aria-label="Demo composer"></textarea><div class="composer-bottom"><button class="demo-icon-button" aria-label="Add attachment" data-toast="Attachment action">${i('plus')}</button><button class="demo-icon-button" aria-label="Choose agent" data-toast="Agent action">${i('agent')}</button><button class="button solid" data-toast="Demo message sent">${i('send',16)} Send</button></div></div>`;
    case 'message': return `<div class="demo-message"><div class="meta"><span>You</span><span>Just now</span></div><div class="bubble">Let's refine the motion of the sidebar icon.</div></div>`;
    case 'diff': return `<div class="demo-diff"><div><span>41</span>const width = 240;</div><div class="del"><span>42</span>- const gap = 12;</div><div class="add"><span>42</span>+ const gap = 8;</div><div><span>43</span>return layout;</div></div>`;
    case 'dialog': return `<div class="demo-actions"><button class="button solid" data-open-dialog>${i('folder-add')} Create a space</button></div>`;
    case 'empty': return `<div class="demo-empty">${i('folder',28)}<strong>No projects yet</strong><p>Add a folder to start a new workspace.</p><button class="button quiet" data-toast="Add project">Add project</button></div>`;
    default: return `<div class="demo-inline">${i(item.icon)} ${escapeHTML(item.name)}</div>`;
  }
}
function renderComponents() {
  const groups = ['All', ...new Set(state.components.map(x => x.group))];
  $('#component-filters').innerHTML = groups.map(group => `<button data-component-group="${escapeHTML(group)}" class="${group === state.componentCategory ? 'active' : ''}" aria-pressed="${group === state.componentCategory}">${escapeHTML(group)}</button>`).join('');
  const query = $('#component-search').value.trim().toLowerCase();
  const matched = state.components.filter(item => (state.componentCategory === 'All' || item.group === state.componentCategory) && `${item.name} ${item.description} ${item.group}`.toLowerCase().includes(query));
  $('#component-result-count').textContent = `${matched.length} / ${state.components.length}`;
  $('#component-empty').hidden = matched.length !== 0;
  $('#component-grid').innerHTML = matched.map(item => `<article class="component-card"><div class="component-preview">${previewFor(item)}</div><div class="component-info"><span class="comp-kicker">${escapeHTML(item.group.toUpperCase())}</span><h3>${escapeHTML(item.name)}</h3><p>${escapeHTML(item.description)}</p><div class="spec-row"><span class="specimen-note">BROWSER SPECIMEN</span>${source(item.source, 'Native source')}</div></div></article>`).join('');
}
function renderPatterns() {
  const i = name => glyph(name,16);
  $('#pattern-content').innerHTML = `<article class="pattern-card"><header><div><h2>Workspace shell</h2><p>One selected session, a quiet sidebar and a stable message plane.</p></div>${source('crates/ui/src/shell.rs', 'shell.rs')}</header><div class="pattern-stage"><div class="workspace-mock"><div class="mock-sidebar"><div class="mock-title">zeron ${i('compose')}</div><div class="mock-space">All spaces ${i('chevron-down')}</div><div class="mock-label">SESSIONS</div><div class="mock-session active"><i class="status-dot busy"></i>UI companion</div><div class="mock-session"><i class="status-dot"></i>Search performance</div><div class="mock-session"><i class="status-dot warn"></i>Design review</div></div><div class="mock-panel"><div class="mock-tabbar">${i('chat')} UI companion <span style="margin-left:auto">${i('split')}</span></div><div class="mock-transcript"><div class="mock-bubble">Show me the design system.</div><div class="mock-response"><strong>Zeron</strong>Here are the foundations, components, and interaction states that make the app feel coherent.</div></div><div class="mock-composer">Ask a follow-up… ${i('send')}</div></div></div></div></article>
  <div class="pattern-two"><article class="pattern-card"><header><div><h2>Settings rows</h2><p>Identity, explanation and one clear control.</p></div>${source('crates/ui/src/settings/widgets.rs','widgets.rs')}</header><div class="pattern-stage"><div class="settings-mock"><h3>Notifications</h3><p>Choose when Zeron gets your attention.</p>${[['bell','Completion sound','Play when an agent finishes'],['volume','Voice feedback','Hear brief status updates'],['moon','Quiet hours','Pause nonessential alerts']].map(([icon,title,subtitle],ix) => `<div class="demo-settings-row"><div class="row-tile">${i(icon)}</div><div><strong>${title}</strong><small>${subtitle}</small></div><button class="demo-switch" role="switch" aria-label="${title}" aria-checked="${ix===0}"></button></div>`).join('')}</div></div></article><article class="pattern-card"><header><div><h2>Conversation composer</h2><p>One elevated plate and a single primary action.</p></div>${source('crates/ui/src/composer.rs','composer.rs')}</header><div class="pattern-stage" style="display:grid;place-items:center"><div style="max-width:520px;width:100%">${previewFor({id:'composer'})}</div></div></article></div>`;
}
function renderIcons() {
  const groups = ['All', ...new Set(state.icons.map(x => x.category))];
  $('#icon-categories').innerHTML = groups.map(group => `<button data-icon-group="${escapeHTML(group)}" aria-pressed="${group === state.iconCategory}" class="${group === state.iconCategory ? 'active' : ''}">${escapeHTML(group)}</button>`).join('');
  const query = $('#icon-search').value.trim().toLowerCase();
  const matched = state.icons.filter(item => (state.iconCategory === 'All' || item.category === state.iconCategory) && `${item.name} ${item.category} ${item.legacy?.join(' ') || ''}`.toLowerCase().includes(query));
  $('#icon-empty').hidden = matched.length !== 0;
  $('#icon-grid').style.setProperty('--icon-preview-size', `${state.iconSize}px`);
  $('#icon-grid').innerHTML = matched.map(item => `<button class="icon-tile" data-inspect-icon="${escapeHTML(item.name)}" title="${escapeHTML(item.name)}">${glyph(item.name, state.iconSize)}<span>${escapeHTML(item.name)}</span></button>`).join('');
  $$('.icon-size button').forEach(button => button.classList.toggle('selected', Number(button.dataset.size) === state.iconSize));
}
function inspectIcon(name) {
  const icon = state.iconMap.get(name); if (!icon) return;
  const refs = icon.references || [];
  $('#icon-dialog-body').innerHTML = `<div class="icon-dialog-preview"><div>${glyph(name,64)}STANDARD 24 GRID</div><div>${glyph(name,16)}OPTICAL 16PX</div></div><div class="icon-detail"><h2>${escapeHTML(name)}</h2><p>${escapeHTML(icon.note || 'Custom Zeron glyph.')}</p><p>${escapeHTML(icon.category)} · ${refs.length} source reference${refs.length === 1 ? '' : 's'} · 1.75 stroke</p>${refs.slice(0,5).map(ref => source(ref, ref)).join('')}<a class="button quiet" href="../icon-lab/svg/${encodeURIComponent(name)}.svg" download="zeron-${escapeHTML(name)}.svg">Download SVG ↗</a></div>`;
  $('#icon-dialog').showModal();
}
function renderMotions() {
  state.motionItems = state.motions.map((spec, index) => {
    const from = state.iconMap.get(spec.fromIcon), to = state.iconMap.get(spec.toIcon);
    const plan = window.ZeronGeometry.plan(from.paths, to.paths, spec);
    return {spec, index, plan, state:new window.ZeronMotionState(0), raf:0};
  });
  $('#motion-grid').innerHTML = state.motionItems.map(({spec}, index) => `<button class="motion-tile" data-motion="${index}" aria-label="Animate ${escapeHTML(spec.title)}"><div class="motion-icon" id="motion-icon-${index}">${glyph(spec.fromIcon,40)}</div><b>${escapeHTML(spec.title)}</b><small>${escapeHTML(spec.fromIcon)} ↔ ${escapeHTML(spec.toIcon)}</small></button>`).join('');
}
function drawMotion(item, value) {
  const host = $(`#motion-icon-${item.index}`); if (!host) return;
  host.innerHTML = pathsSVG(window.ZeronGeometry.frame(item.plan, value),40);
}
function reduceMotion() { return $('#reduce-motion').checked || window.matchMedia('(prefers-reduced-motion: reduce)').matches; }
function toggleMotion(index) {
  const item = state.motionItems[index]; if (!item) return;
  cancelAnimationFrame(item.raf);
  const next = item.state.target === 0 ? 1 : 0;
  if (reduceMotion()) { item.state.snap(next); drawMotion(item,next); return; }
  item.state.retarget(next, performance.now(), item.spec.duration);
  const tick = now => { drawMotion(item,item.state.sample(now,item.spec.duration)); if(item.state.active)item.raf=requestAnimationFrame(tick); };
  item.raf = requestAnimationFrame(tick);
}
function scrubMotions(value) {
  const t = Number(value)/100;
  $('#motion-value').textContent = `${value}%`;
  for (const item of state.motionItems) { cancelAnimationFrame(item.raf); item.state.snap(t); drawMotion(item,t); }
}
function attachEvents() {
  window.addEventListener('hashchange', navigate);
  document.addEventListener('click', event => {
    const target = event.target.closest('button, a'); if (!target) return;
    if (target.matches('[data-theme]')) { state.appearance = target.dataset.theme; applyTheme(); renderFoundations(); }
    if (target.matches('[data-component-group]')) { state.componentCategory=target.dataset.componentGroup; renderComponents(); }
    if (target.matches('[data-icon-group]')) { state.iconCategory=target.dataset.iconGroup; renderIcons(); }
    if (target.matches('[data-size]')) { state.iconSize=Number(target.dataset.size); renderIcons(); }
    if (target.matches('[data-inspect-icon]')) inspectIcon(target.dataset.inspectIcon);
    if (target.matches('[data-motion]')) toggleMotion(Number(target.dataset.motion));
    if (target.matches('[data-toast]')) showToast(target.dataset.toast);
    if (target.matches('[data-open-dialog]')) $('#demo-dialog').showModal();
    if (target.matches('[data-close-dialog]')) target.closest('dialog')?.close();
    if (target.matches('.demo-switch')) target.setAttribute('aria-checked', String(target.getAttribute('aria-checked') !== 'true'));
    if (target.matches('[data-segment]')) { $$('[data-segment]', target.parentNode).forEach(b => b.setAttribute('aria-pressed', String(b === target))); }
    if (target.matches('[data-tab]')) { $$('[data-tab]', target.parentNode).forEach(b => b.setAttribute('aria-selected', String(b === target))); }
  });
  $('#accent-select').addEventListener('change', event => { state.accent = event.target.value; applyTheme(); renderFoundations(); });
  $('#component-search').addEventListener('input', renderComponents);
  $('#icon-search').addEventListener('input', renderIcons);
  $('#motion-scrub').addEventListener('input', event => scrubMotions(event.target.value));
  $('#reduce-motion').addEventListener('change', () => { if (reduceMotion()) state.motionItems.forEach(item => {cancelAnimationFrame(item.raf);item.state.snap(item.state.target);drawMotion(item,item.state.value);}); });
  $('#create-space').addEventListener('click', () => { $('#demo-dialog').close(); showToast(`${$('#demo-space-name').value.trim() || 'New space'} created in specimen`); });
  $$('dialog').forEach(dialog => dialog.addEventListener('click', event => { if (event.target === dialog) dialog.close(); }));
}
async function boot() {
  const response = await Promise.all(files.map(file => fetch(file).then(r => {if(!r.ok)throw Error(`${file}: ${r.status}`);return r.json();})));
  [state.guide,state.components,state.icons,state.motions] = response;
  state.iconMap = new Map(state.icons.map(item => [item.name,item]));
  if (!state.guide.accentPresets[state.accent]) state.accent='zeron';
  $('#accent-select').innerHTML = Object.keys(state.guide.accentPresets).map(name => `<option value="${name}">${name[0].toUpperCase()+name.slice(1)} accent</option>`).join('');
  applyTheme(); renderOverview(); renderFoundations(); renderComponents(); renderPatterns(); renderIcons(); renderMotions(); attachEvents(); navigate();
}
boot().catch(error => { console.error(error); $('#main').innerHTML = `<div class="page-heading"><h1>Couldn’t load the companion</h1><p>${escapeHTML(error.message)}. Serve the repository root with <code>python3 -m http.server 8767</code>, then open <code>/apps/ui-companion/</code>.</p></div>`; });
