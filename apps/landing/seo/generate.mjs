import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { agents } from './agents.mjs';
import { about, aboutContent } from './about.mjs';

export const root = fileURLToPath(new URL('../', import.meta.url));
export const origin = 'https://zeron.sh';
export const paths = ['/', '/about/', '/agents/', ...agents.map(a => `/agents/${a.slug}/`)];
const escape = value => String(value).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const link = a => `/agents/${a.slug}/`;
const icon = a => `<span class="agent-icon" data-harness="${a.slug === 'claude-code' ? 'claude' : a.slug}" aria-hidden="true" style="--icon:url('/assets/icons/${a.icon}-mark.svg')"></span>`;
const cards = items => `<div class="agent-directory">${items.map(a => `<a class="agent-entry" href="${link(a)}"><span class="agent-entry-title">${icon(a)}<b>${a.name}</b><span class="agent-entry-arrow" aria-hidden="true"><span class="ui-icon" data-icon="arrow-up-right" aria-hidden="true"></span></span></span><span class="agent-entry-copy">${escape(a.heading)}</span></a>`).join('')}</div>`;
const screenshot = `<section class="shot"><div class="wrap"><figure><div class="showcase-frame"><img class="showcase-image showcase-image--dark" src="/assets/shots/showcase-dark.webp" width="2582" height="1696" alt="Zeron’s shared desktop interface with an agent conversation, live website preview, and repository files." fetchpriority="high"><img class="showcase-image showcase-image--light" src="/assets/shots/showcase-light.webp" width="2582" height="1696" alt="Zeron’s shared desktop interface with an agent conversation, live website preview, and repository files." fetchpriority="high"></div><figcaption class="agent-shot-note">The Zeron workspace. Shown for illustration; available controls vary by harness.</figcaption></figure></div></section>`;
const cta = `<div class="cta-row"><a class="btn" id="hero-download" href="/#downloads">Download Zeron</a><a class="text-link" href="/#downloads">All downloads <span class="ui-icon" data-icon="arrow-down" aria-hidden="true"></span></a><span class="cta-ver" id="ver">v0.2.66</span></div><p class="open-source-note">Free and open source. <a href="https://github.com/zeronsh/zeron/blob/main/LICENSE">MIT licensed.</a> <a href="https://github.com/zeronsh/zeron">View source <span class="ui-icon" data-icon="arrow-up-right" aria-hidden="true"></span></a></p><p class="agent-platforms">macOS · Windows · Linux <span aria-hidden="true">·</span> iOS coming soon</p>`;
const sectionHead = (lead, copy) => `<div class="sec-head"><h2><b>${lead}</b> ${copy}</h2></div>`;
const hero = (heading, intro, breadcrumb = '') => `<header class="hero"><div class="wrap hero-grid"><div class="hero-copy">${breadcrumb}<h1><b>${heading}</b> ${intro}</h1>${cta}</div></div></header>`;
const faq = a => [
  [a.question, a.answer],
  [`Can I control ${a.name} from another machine?`, `Yes, with Zeron’s optional multi-device sync. Run the engine and ${a.name} integration on the execution machine, and use another trusted device signed into the same synced account to follow or drive its work. The host must stay running and connected.`],
  ['Do I need a Zeron account?', 'No account is required for local-only use. Sign in when you want optional device sync. Agent authentication is separate, and provider subscriptions or API usage may still cost money. Local-only Zeron storage does not mean the agent’s model runs offline.'],
  ['Can I use it on my phone?', 'The Zeron iOS app is coming soon. The currently listed desktop downloads are for macOS, Windows, and Linux.'],
];
function pageContent(a, gallery) {
  const breadcrumb = `<nav class="agent-breadcrumb" aria-label="Breadcrumb"><a href="/agents/">All agents</a><span aria-hidden="true">/</span><span aria-current="page">${icon(a)} ${a.name}</span></nav>`;
  return `${hero(a.heading, a.intro, breadcrumb)}${screenshot}
<section class="panel-sect"><div class="wrap">${sectionHead(a.angle + '.', a.detail)}${gallery}</div></section>
<section class="agent-section"><div class="wrap">${sectionHead('Run ' + a.name + ' in Zeron.', 'Start with a local workspace. Connect your other machines when you need them.')}<ol class="agent-steps"><li><span class="agent-step-number" aria-hidden="true">01</span><h3>Install Zeron.</h3><p>Download Zeron for the machine where your repository lives. Local use does not require a Zeron account.</p><a class="text-link" href="/#downloads">Choose your download <span class="ui-icon" data-icon="arrow-up-right" aria-hidden="true"></span></a></li><li><span class="agent-step-number" aria-hidden="true">02</span><h3>Connect ${a.name}.</h3><p>${a.setup}</p><a class="text-link" href="${a.docs}">Read ${a.name} documentation <span class="ui-icon" data-icon="arrow-up-right" aria-hidden="true"></span></a></li><li><span class="agent-step-number" aria-hidden="true">03</span><h3>Follow the work.</h3><p>Send a task, follow the conversation and tool activity, and inspect the branch diff. Keep the agent’s credentials configured on the execution machine.</p></li></ol></div></section>
<section class="agent-section"><div class="wrap agent-reading"><h2><b>The agent you chose.</b> The interface you share.</h2><div><p>${a.integration}</p><p>Zeron’s desktop interface is built in Rust with GPUI. The project is open source and MIT licensed. Agent-specific tools and controls depend on what each integration exposes.</p></div></div></section>
<section class="agent-section"><div class="wrap agent-faq">${sectionHead('A few things to know.', 'Using ' + a.name + ' with Zeron.')} ${faq(a).map(([q,ans]) => `<details><summary>${q}<span class="agent-disclosure ui-icon" data-icon="plus" aria-hidden="true"></span></summary><p>${ans}</p></details>`).join('')}</div></section>
<section class="agent-section agent-related"><div class="wrap">${sectionHead('One workspace. More agents.', 'Keep the same interface when you switch harnesses.')}${cards(agents.filter(other => other.slug !== a.slug))}</div></section>`;
}
export async function generatePages() {
  const home = await readFile(resolve(root, 'index.html'), 'utf8');
  const theme = home.match(/<script>\s*\(\(\) => \{[\s\S]*?<\/script>/)[0];
  // Reuse the landing page chrome and media markup, so visual changes propagate.
  const nav = home.match(/<nav class="site-nav"[\s\S]*?<\/nav>/)[0]
    .replaceAll('href="#downloads"', 'href="/#downloads"');
  const footer = home.match(/<div class="footer-reveal-space"[\s\S]*?<\/footer>/)[0]
    .replaceAll('href="#downloads"', 'href="/#downloads"');
  const figures = [...home.matchAll(/<figure>[\s\S]*?<\/figure>/g)].slice(0, 3).map(m => m[0]).join('');
  const gallery = `<div class="panel"><i class="pdot tl"></i><i class="pdot tr"></i><i class="pdot bl"></i><i class="pdot br"></i><div class="panel-row p-gallery">${figures}</div></div>`;
  const entries = [ {slug: '', title: 'Coding Agent GUIs & Desktop Integrations | Zeron', description: 'Explore Zeron’s native desktop integrations for Claude Code, Cursor, Codex, OpenCode, Antigravity, Devin, Hermes, Pi, and Grok.', content: `${hero('Choose your agent. Keep your workspace.', 'Start locally. Connect your machines when you need to.')}<section class="agent-index-directory"><div class="wrap">${cards(agents)}</div></section>`}, ...agents.map(a => ({...a, content: pageContent(a, gallery)})) ];
  entries.push({...about, content: aboutContent(hero)});
  for (const entry of entries) {
    const path = entry.path || `/agents/${entry.slug ? entry.slug + '/' : ''}`;
    const breadcrumbs = entry.path ? [{name:'Zeron',item:origin+'/'},{name:entry.name,item:origin+path}] : [{name:'Zeron',item:origin+'/'},{name:'Agents',item:origin+'/agents/'}, ...(entry.slug ? [{name:entry.name,item:origin+path}] : [])];
    const schema = {'@context':'https://schema.org','@graph':[
      {'@type':'WebPage','@id':origin+path,url:origin+path,name:entry.title,description:entry.description,inLanguage:'en',isPartOf:{'@type':'WebSite',name:'Zeron',url:origin+'/' }},
      {'@type':'BreadcrumbList',itemListElement:breadcrumbs.map((b,i)=>({'@type':'ListItem',position:i+1,...b}))},
    ]};
    const html = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>${escape(entry.title)}</title><meta name="description" content="${escape(entry.description)}"><link rel="canonical" href="${origin+path}"><meta property="og:type" content="website"><meta property="og:site_name" content="Zeron"><meta property="og:title" content="${escape(entry.title)}"><meta property="og:description" content="${escape(entry.description)}"><meta property="og:url" content="${origin+path}"><meta property="og:image" content="${origin}/assets/shots/showcase-dark.webp"><meta name="twitter:card" content="summary_large_image"><meta name="twitter:title" content="${escape(entry.title)}"><meta name="twitter:description" content="${escape(entry.description)}"><meta name="twitter:image" content="${origin}/assets/shots/showcase-dark.webp"><link rel="icon" href="/assets/brand/zeron-favicon.svg" type="image/svg+xml">${theme}<script type="application/ld+json">${JSON.stringify(schema).replace(/</g,'\\u003c')}</script><script type="module" src="/src/agents.js"></script></head><body class="agent-page"><a class="agent-skip" href="#content">Skip to content</a>${entry.path ? nav : nav.replace('href="/agents/"', 'href="/agents/" aria-current="true"')}<main id="content" class="page-shell">${entry.content}</main>${footer}</body></html>`;
    const dir = resolve(root, '.' + path);
    await mkdir(dir,{recursive:true});
    await writeFile(resolve(dir,'index.html'),html+'\n');
  }
}
export const sitemap = `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">${paths.map(p=>`<url><loc>${origin+p}</loc></url>`).join('')}</urlset>\n`;
