import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';

gsap.registerPlugin(ScrollTrigger);
const media = gsap.matchMedia();
media.add({ motion: '(prefers-reduced-motion: no-preference)', mobile: '(max-width: 900px), (pointer: coarse)' }, ({ conditions }) => {
  if (!conditions.motion) return;
  // Content is visible in CSS. Only one-off entrances use opacity; the quote
  // uses a scroll-linked word fill on the plain page surface.
  const heroItems = gsap.utils.toArray('.hero h1, .hero .cta-row, .used-by');
  if (window.scrollY < 80) {
    gsap.from(heroItems, {
      y: 14, opacity: 0, duration: .7, stagger: .1,
      ease: 'power2.out', clearProps: 'transform,opacity',
    });
  }
  if (document.querySelector('.showcase-frame')) {
  gsap.from('.showcase-frame', {
    y: 24, scale: .985, duration: 1, ease: 'power2.out',
    clearProps: 'transform',
    scrollTrigger: { trigger: '.shot', start: 'top 92%', once: true },
  });
  }
  for (const selector of ['.p-gallery', '.downloads']) {
    const section = document.querySelector(selector);
    if (!section) continue;
    const items = section.querySelectorAll(selector === '.p-gallery' ? 'figure' : '.sec-head, .dl h3, .dl-list li, .download-foot');
    gsap.from(items, {
      y: 18, opacity: 0, duration: .65, stagger: .09,
      ease: 'power2.out', clearProps: 'transform,opacity',
      scrollTrigger: { trigger: section, start: 'top 88%', once: true },
    });
  }

  const quote = document.querySelector('.plate-quote p');
  if (quote) {
    const original = quote.textContent;
    const words = original.split(/(\s+)/).map(text => {
      if (/^\s+$/.test(text)) return document.createTextNode(text);
      const word = document.createElement('span');
      word.className = 'quote-word';
      word.textContent = text;
      return word;
    });
    quote.replaceChildren(...words);
    const fillWords = quote.querySelectorAll('.quote-word');
    gsap.set(fillWords, { '--fill': '0%' });
    gsap.to(fillWords, {
      '--fill': '100%', duration: 1, stagger: 1, ease: 'none',
      scrollTrigger: {
        trigger: quote, start: 'top 82%', end: 'bottom 48%',
        scrub: conditions.mobile ? true : .35,
      },
    });
    return () => { quote.textContent = original; };
  }

});

// Font swaps and late content can move the quote after ScrollTrigger's first
// measurement. Refresh on document geometry changes, not just viewport resize.
let refreshFrame = 0;
let disposed = false;
const refreshLayout = () => {
  if (disposed || refreshFrame) return;
  refreshFrame = requestAnimationFrame(() => {
    refreshFrame = 0;
    ScrollTrigger.refresh();
  });
};
const shell = document.querySelector('.page-shell');
let previousSize = '';
const layoutObserver = new ResizeObserver(([entry]) => {
  const size = `${entry.contentRect.width}:${entry.contentRect.height}`;
  if (size === previousSize) return;
  previousSize = size;
  refreshLayout();
});
if (shell) layoutObserver.observe(shell);
document.fonts.ready.then(refreshLayout);
window.addEventListener('load', refreshLayout, { once: true });
if (import.meta.hot) import.meta.hot.dispose(() => {
  disposed = true;
  cancelAnimationFrame(refreshFrame);
  layoutObserver.disconnect();
  window.removeEventListener('load', refreshLayout);
  media.revert();
});
