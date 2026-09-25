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
  syncGlyphFields();
});

systemTheme.addEventListener('change', () => {
  let saved;
  try {
    saved = localStorage.getItem('zeron-landing-theme');
  } catch {}
  if (saved !== 'light' && saved !== 'dark') {
    document.documentElement.dataset.theme = systemTheme.matches ? 'light' : 'dark';
    syncThemeButton();
    syncGlyphFields();
  }
});
syncThemeButton();

const navDownload = document.getElementById('nav-download');
const hero = document.querySelector('.hero');
const nav = document.querySelector('.site-nav');
let navUpdateQueued = false;
function syncNavDownload() {
  navUpdateQueued = false;
  navDownload.hidden = hero.getBoundingClientRect().bottom > nav.getBoundingClientRect().bottom;
}
function queueNavUpdate() {
  if (navUpdateQueued) return;
  navUpdateQueued = true;
  requestAnimationFrame(syncNavDownload);
}
addEventListener('scroll', queueNavUpdate, { passive: true });
addEventListener('resize', queueNavUpdate);
syncNavDownload();

const showcaseVideo = document.querySelector('.showcase-video');
if (showcaseVideo) {
  showcaseVideo.muted = true;
  showcaseVideo.defaultPlaybackRate = 1.25;
  showcaseVideo.playbackRate = 1.25;
  const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
  let inView = true;
  const syncVideo = () => {
    if (inView && !reducedMotion.matches) showcaseVideo.play().catch(() => {});
    else showcaseVideo.pause();
  };
  if ('IntersectionObserver' in window) {
    inView = false;
    new IntersectionObserver(([entry]) => {
      inView = entry.isIntersecting;
      syncVideo();
    }, { threshold: 0.1 }).observe(showcaseVideo);
  } else syncVideo();
  reducedMotion.addEventListener('change', syncVideo);
}

const heroPalette = () => document.documentElement.dataset.theme === 'light'
  ? {
      base: [75, 62, 94], baseAlpha: [0.055, 0.12],
      glow: [105, 75, 169], glowAlpha: 0.28,
      beam: { angle: -38, width: 0.12, sweep: 26, alpha: 0.16 },
    }
  : {
      base: [156, 146, 181], baseAlpha: [0.05, 0.13],
      glow: [183, 157, 249], glowAlpha: 0.6,
      beam: { angle: -38, width: 0.12, sweep: 26, alpha: 0.3 },
    };

const heroField = mountGlyphField(document.querySelector('.hero'), {
  ...heroPalette(),
  clearElement: document.querySelector('.hero-copy'),
});

const footerPalette = () => document.documentElement.dataset.theme === 'light'
  ? {
      base: [75, 62, 94], baseAlpha: [0.035, 0.075],
      glow: [105, 75, 169],
      beam: { angle: -38, width: 0.14, sweep: 34, alpha: 0.11 },
    }
  : {
      base: [156, 146, 181], baseAlpha: [0.035, 0.085],
      glow: [183, 157, 249],
      beam: { angle: -38, width: 0.14, sweep: 34, alpha: 0.2 },
    };

const footerField = mountGlyphField(document.querySelector('.footer-frame'), {
  ...footerPalette(),
  density: 0.42,
  parallax: 1,
  pointerTarget: document.querySelector('.footer-frame'),
  clearElements: [document.querySelector('.footer-intro h2'), document.querySelector('.footer-intro-copy')],
  clearFeather: 44,
});

function syncGlyphFields() {
  heroField.setAppearance(heroPalette());
  footerField.setAppearance(footerPalette());
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
