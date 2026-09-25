# Zeron landing site

The landing page and brand styleguide are Vite entry points. Static images,
fonts, and icons live in `public/assets` and are copied into the build.

```sh
npm ci
npm run dev
npm test
npm run build
npm run preview
```

`npm run build` writes both pages to `dist/`. Wrangler serves that directory;
build before deploying with `npx wrangler deploy` from this directory.

The page source is `index.html`, `styleguide.html`, and `src/`. Theme choice is
shared between pages through local storage. The landing footer remains fixed
behind the document and is revealed during the final viewport of scrolling.
