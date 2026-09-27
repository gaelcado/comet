import './style.css';
import './telemetry.js';
import './downloads.js';
import './footer-motion.js';
import './collaboration-network.js';
import './page-motion.js';
import { mountGlyphField } from './glyph-field.js';

const themeButton = document.querySelector('.theme-toggle');
const systemTheme = matchMedia('(prefers-color-scheme: light)');
let themeSwapFrame;
function applyTheme(theme) {
  const root = document.documentElement;
  cancelAnimationFrame(themeSwapFrame);
  root.classList.add('theme-changing');
  root.dataset.theme = theme;
  void root.offsetWidth;
  themeSwapFrame = requestAnimationFrame(() => root.classList.remove('theme-changing'));
}

function syncThemeButton() {
  const next = document.documentElement.dataset.theme === 'light' ? 'dark' : 'light';
  themeButton.setAttribute('aria-label', `Switch to ${next} mode`);
  themeButton.title = `Switch to ${next} mode`;
}

themeButton.addEventListener('click', () => {
  const current = document.documentElement.dataset.theme;
  applyTheme(current === 'light' ? 'dark' : 'light');
  try {
    localStorage.setItem('zeron-landing-theme', document.documentElement.dataset.theme);
  } catch {}
  syncThemeButton();
  syncGlyphFields();
});

systemTheme.addEventListener('change', () => {
  let saved;
  try {
    saved = localStorage.getItem('zeron-landing-theme');
  } catch {}
  if (saved !== 'light' && saved !== 'dark') {
    applyTheme(systemTheme.matches ? 'light' : 'dark');
    syncThemeButton();
    syncGlyphFields();
  }
});
syncThemeButton();

const navDownload = document.getElementById('nav-download');
const navDownloadSlot = navDownload.closest('.nav-download-slot');
const hero = document.querySelector('.hero');
const nav = document.querySelector('.site-nav');
let navUpdateQueued = false;
function syncNavDownload() {
  navUpdateQueued = false;
  const visible = hero.getBoundingClientRect().bottom <= nav.getBoundingClientRect().bottom;
  navDownloadSlot.dataset.visible = String(visible);
  navDownloadSlot.inert = !visible;
}
function queueNavUpdate() {
  if (navUpdateQueued) return;
  navUpdateQueued = true;
  requestAnimationFrame(syncNavDownload);
}
addEventListener('scroll', queueNavUpdate, { passive: true });
addEventListener('resize', queueNavUpdate);
syncNavDownload();

const glyphPalette = () => document.documentElement.dataset.theme === 'light'
  ? {
      base: [75, 62, 94], baseAlpha: [0.065, 0.095],
      glow: [105, 75, 169], glowAlpha: 0.18,
      beam: { angle: -38, width: 0.12, sweep: 32, alpha: 0.06 },
    }
  : {
      base: [156, 146, 181], baseAlpha: [0.06, 0.095],
      glow: [183, 157, 249], glowAlpha: 0.3,
      beam: { angle: -38, width: 0.12, sweep: 32, alpha: 0.09 },
    };

const heroField = mountGlyphField(document.querySelector('.page-shell'), {
  ...glyphPalette(),
  endElement: document.querySelector('.showcase-frame'),
  endFraction: 0.8,
  clearTextElements: [...document.querySelectorAll('.open-source-note, .sponsor-chip, .about-story, .hero h1, .hero .cta-row .text-link, .hero .cta-ver, .used-by p, .agent-platforms, .agent-breadcrumb, .agent-entry-title, .agent-entry-copy')],
  clearElements: [...document.querySelectorAll('.used-by-logos a')],
  clearFeather: 20,
});

const footerField = mountGlyphField(document.querySelector('.footer-frame'), {
  ...glyphPalette(),
  parallax: 1,
  pointerTarget: document.querySelector('.footer-frame'),
  clearTextElements: [...document.querySelectorAll('.footer-intro h2, .footer-intro p, .footer-intro .text-link')],
  clearFeather: 20,
});

function syncGlyphFields() {
  heroField.setAppearance(glyphPalette());
  footerField.setAppearance(glyphPalette());
}

// Duplicate each tweet column to make its drift loop seamlessly.
const wall = document.querySelector('.wall');
if (wall) {
  for (const col of wall.querySelectorAll('.wall-col')) {
    const duplicate = document.createElement('div');
    duplicate.style.display = 'contents';
    duplicate.setAttribute('aria-hidden', 'true');
    for (const card of col.children) {
      const copy = card.cloneNode(true);
      copy.tabIndex = -1;
      duplicate.append(copy);
    }
    col.append(duplicate);
  }
  wall.classList.add('is-live');
}

// Cache GitHub stars for an hour to limit unauthenticated API requests.
const starsKey = 'zeron-gh-stars';
function showStars(count) {
  const label = new Intl.NumberFormat('en', {
    notation: 'compact',
    maximumFractionDigits: 1,
  }).format(count).toLowerCase();
  for (const element of document.querySelectorAll('.gh-stars')) {
    element.textContent = label;
    element.hidden = false;
    if (element.closest('.site-nav')) {
      element.closest('a').setAttribute('aria-label', `GitHub, ${count.toLocaleString('en')} stars`);
    }
  }
}

let cachedStars = null;
try {
  cachedStars = JSON.parse(localStorage.getItem(starsKey));
} catch {}
if (cachedStars && Number.isFinite(cachedStars.n)) showStars(cachedStars.n);
if (!cachedStars || Date.now() - cachedStars.at >= 3600e3) {
  fetch('https://api.github.com/repos/zeronsh/zeron', { credentials: 'omit' })
    .then((response) => response.ok ? response.json() : Promise.reject())
    .then(({ stargazers_count: count }) => {
      if (!Number.isFinite(count)) return;
      showStars(count);
      try {
        localStorage.setItem(starsKey, JSON.stringify({ n: count, at: Date.now() }));
      } catch {}
    })
    .catch(() => {});
}
