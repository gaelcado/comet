# Zeron landing site

The landing page is a Vite entry point. Static images, fonts, and icons live
in `public/assets` and are copied into the build.

```sh
npm ci
npm run dev
npm test
npm run build
npm run preview
```

`npm run build` writes the page to `dist/`. Wrangler serves that directory;
build before deploying with `npx wrangler deploy` from this directory.

The page source is `index.html` and `src/`. Theme choice is stored in local
storage. The landing footer remains fixed behind the document and is revealed
during the final viewport of scrolling.

## Agent pages and SEO

Edit `seo/agents.mjs` for the nine harness pages. Vite generates the ignored
`agents/` directory on startup and builds those pages plus `/agents/` as static
HTML. `src/agents.css` extends the landing layout; `src/agents.js` loads the shared
landing runtime. The generator reuses homepage nav, gallery, and footer markup.
Run `npm run test:seo` to check the built routes, content, metadata, structured
data, links, assets, and sitemap. Keyword research is in `docs/seo-keywords.md`.

Canonicals and the sitemap use `https://zeron.sh`. Vercel preview hosts receive
`X-Robots-Tag: noindex`; production indexing requires publishing `dist/` to the
canonical domain. No search-engine submission is performed by the build.
