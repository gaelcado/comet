import { syncNativeIcons } from './seo/icons.mjs';
import { defineConfig } from 'vite';
import { resolve } from 'node:path';
import { agents } from './seo/agents.mjs';
import { generatePages, root, sitemap, origin } from './seo/generate.mjs';

export default defineConfig(async () => {
  await syncNativeIcons();
  await generatePages();
  return {
    build: {
      rollupOptions: {
        input: [resolve(root, 'index.html'), resolve(root, 'agents/index.html'), resolve(root, 'about/index.html'),
          ...agents.map(a => resolve(root, `agents/${a.slug}/index.html`))],
      },
    },
    plugins: [{
      name: 'zeron-seo',
      generateBundle() {
        this.emitFile({type:'asset',fileName:'sitemap.xml',source:sitemap});
        this.emitFile({type:'asset',fileName:'robots.txt',source:`User-agent: *\nAllow: /\nSitemap: ${origin}/sitemap.xml\n`});
      },
    }],
  };
});
