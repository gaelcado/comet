import './style.css';
import './telemetry.js';
import './downloads.js';
import './footer-motion.js';
import { mountGlyphField } from './glyph-field.js';

const themeButton = document.querySelector('.theme-toggle');
const systemTheme = matchMedia('(prefers-color-scheme: light)');

function syncThemeButton() {
  const next = document.documentElement.dataset.theme === 'light' ? 'dark' : 'light';
  themeButton.setAttribute('aria-label', `Switch to ${next} mode`);
  themeButton.title = `Switch to ${next} mode`;
}

themeButton.addEventListener('click', () => {
  const current = document.documentElement.dataset.theme;
  document.documentElement.dataset.theme = current === 'light' ? 'dark' : 'light';
  try {
    localStorage.setItem('zeron-landing-theme', document.documentElement.dataset.theme);
  } catch {}
  syncThemeButton();
});

systemTheme.addEventListener('change', () => {
  let saved;
  try {
    saved = localStorage.getItem('zeron-landing-theme');
  } catch {}
  if (saved !== 'light' && saved !== 'dark') {
    document.documentElement.dataset.theme = systemTheme.matches ? 'light' : 'dark';
    syncThemeButton();
  }
});
syncThemeButton();

// Match Zeron's 24 px scroll edge ramps: an edge fades only while content
// remains more than 1 px beyond it. Keep the fixed navigation above the ramp.
const pageShell = document.querySelector('.page-shell');
const nav = document.querySelector('.site-nav');
const topFade = document.querySelector('.scroll-edge-fade.top');
const bottomFade = document.querySelector('.scroll-edge-fade.bottom');
let fadeFrame = 0;

function syncScrollEdges() {
  fadeFrame = 0;
  const shellBottom = pageShell.getBoundingClientRect().bottom;
  const navBottom = nav.getBoundingClientRect().bottom;
  topFade.dataset.active = String(window.scrollY > 1 && shellBottom > navBottom + 1);
  bottomFade.dataset.active = String(shellBottom > window.innerHeight + 1);
}

function scheduleScrollEdges() {
  if (!fadeFrame) fadeFrame = requestAnimationFrame(syncScrollEdges);
}

window.addEventListener('scroll', scheduleScrollEdges, { passive: true });
window.addEventListener('resize', scheduleScrollEdges);
syncScrollEdges();

const showcaseVideo = document.querySelector('.showcase-video');
if (showcaseVideo && 'IntersectionObserver' in window) {
  const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
  const observer = new IntersectionObserver(([entry]) => {
    if (entry.isIntersecting && !reducedMotion.matches) {
      showcaseVideo.play().catch(() => {});
    } else {
      showcaseVideo.pause();
    }
  }, { threshold: 0.1 });
  observer.observe(showcaseVideo);
}

// Faint agent traces and a slow beam sit outside a clear ellipse for the hero copy.
mountGlyphField(document.querySelector('.hero'), {
  base: [156, 146, 181],
  baseAlpha: [0.05, 0.13],
  glow: [183, 157, 249],
  glowAlpha: 0.8,
  clear: { x: 0.5, y: 0.5, rx: 0.36, ry: 0.44 },
  beam: { angle: -38, width: 0.12, sweep: 26, alpha: 0.35 },
});

mountGlyphField(document.querySelector('.plate-quote'), {
  baseAlpha: [0.02, 0.05],
  density: 0.42,
  parallax: 1,
  clear: { x: 0.5, y: 0.3, rx: 0.4, ry: 0.3 },
  beam: { angle: -30, width: 0.14, sweep: 30, alpha: 0.25 },
});

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
  const label = '★ ' + new Intl.NumberFormat('en', {
    notation: 'compact',
    maximumFractionDigits: 1,
  }).format(count).toLowerCase();
  for (const element of document.querySelectorAll('.gh-stars')) {
    element.textContent = label;
    element.hidden = false;
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
