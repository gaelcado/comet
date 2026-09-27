import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile, access } from 'node:fs/promises';
import { resolve } from 'node:path';
import { nativeIcons } from './seo/icons.mjs';
import { agents } from './seo/agents.mjs';
import { root, paths, origin } from './seo/generate.mjs';
const dist = resolve(root, 'dist');
const pages = new Map(await Promise.all(paths.map(async path => [path, await readFile(resolve(dist, '.' + path, 'index.html'), 'utf8')])));

test('all integrations are discoverable from the homepage through the directory', () => {
  assert.match(pages.get('/'), /href="\/agents\/"/);
  for (const a of agents) assert.ok(pages.get('/agents/').includes(`href="/agents/${a.slug}/"`));
});
test('built pages have unique metadata, one H1 and production canonicals', () => {
  const titles = new Set(), descriptions = new Set();
  for (const [path, html] of pages) {
    assert.equal((html.match(/<h1(?:\s|>)/g) || []).length, 1, path);
    const title = html.match(/<title>(.*?)<\/title>/s)?.[1];
    const description = html.match(/<meta name="description" content="([^"]+)"/)?.[1];
    assert.ok(title && description, path);
    assert.ok(!titles.has(title) && !descriptions.has(description), path);
    titles.add(title); descriptions.add(description);
    assert.ok(html.includes(`rel="canonical" href="${origin}${path}"`), path);
  }
});
test('agent content and valid structured data are present without JavaScript', () => {
  for (const a of agents) {
    const path = `/agents/${a.slug}/`, html = pages.get(path);
    assert.ok(html.includes(a.heading), path);
    assert.ok(html.includes(a.integration), path);
    assert.ok(html.includes('iOS app is coming soon'), path);
    assert.ok(html.includes('Shown for illustration'), path);
    assert.equal((html.match(/<details>/g) || []).length, 4, path);
    const schema = JSON.parse(html.match(/<script type="application\/ld\+json">(.*?)<\/script>/s)[1]);
    assert.equal(schema['@graph'][0].url, origin + path);
    assert.equal(schema['@graph'][1].itemListElement.at(-1).item, origin + path);
  }
});
test('internal links, fragments, scripts, and images resolve in the built output', async () => {
  for (const [path, html] of pages) {
    for (const match of html.matchAll(/(?:href|src)="(\/[^"?]*)"/g)) {
      const [url, fragment] = match[1].split('#');
      if (pages.has(url)) {
        if (fragment) assert.ok(pages.get(url).includes(`id="${fragment}"`), `${path}: ${match[1]}`);
      } else await access(resolve(dist, '.' + url));
    }
  }
});
test('sitemap contains every canonical page exactly once', async () => {
  const sitemap = await readFile(resolve(dist, 'sitemap.xml'), 'utf8');
  assert.deepEqual([...sitemap.matchAll(/<loc>(.*?)<\/loc>/g)].map(m => m[1]), paths.map(p => origin + p));
  assert.ok((await readFile(resolve(dist, 'robots.txt'), 'utf8')).includes(`Sitemap: ${origin}/sitemap.xml`));
});

test('generated pages provide the markup required by the shared landing runtime', () => {
  for (const [path, html] of pages) {
    if (path === '/') continue;
    for (const id of ['hero-download', 'nav-download', 'closing-download', 'ver']) {
      assert.equal(html.split(`id="${id}"`).length - 1, 1, `${path}: ${id}`);
    }
    for (const className of ['page-shell', 'hero', 'footer-reveal-space', 'footer-frame', ...(path === '/agents/' || path === '/about/' ? [] : ['shot', 'showcase-frame'])]) {
      assert.ok(html.includes(`class="${className}"`), `${path}: ${className}`);
    }
    assert.ok(html.includes('class="nav-download-slot"'), path);
    assert.ok(html.includes('class="sun"') && html.includes('class="moon"'), path);
    assert.ok(!html.includes('class="agent-card"'), path);
    assert.ok(html.includes('href="/#downloads"'), path);
  }
});

test('sponsor chip uses verified sponsor avatars and links to GitHub Sponsors', () => {
  const home = pages.get('/');
  assert.ok(home.indexOf('class="sponsor-chip"') < home.indexOf('<h1>'));
  assert.ok(home.includes('href="https://github.com/sponsors/zeronsh"'));
  for (const avatar of ['rauchg.jpg', 'the-context-company.png']) assert.ok(home.includes(`/assets/sponsors/${avatar}`));
  for (const html of pages.values()) assert.ok(html.includes('href="/about/"'));
});

test('website icon assets match the native app SVGs exactly', async () => {
  for (const name of nativeIcons) {
    const source = await readFile(resolve(root, '../../crates/ui/assets/icons', `${name}.svg`));
    const built = await readFile(resolve(dist, 'assets/icons/native', `${name}.svg`));
    assert.deepEqual(built, source, name);
  }
  for (const html of pages.values()) assert.ok(!/[↗↓★]/.test(html));
});
