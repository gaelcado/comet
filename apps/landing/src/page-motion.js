import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';

gsap.registerPlugin(ScrollTrigger);
const media = gsap.matchMedia();
media.add({ motion: '(prefers-reduced-motion: no-preference)', mobile: '(max-width: 900px), (pointer: coarse)' }, ({ conditions }) => {
  if (!conditions.motion) return;
  // Content is visible in CSS. Only one-off entrances use opacity; the quote
  // remains readable throughout its scroll-linked architectural reveal.
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

  const review = document.querySelector('.plate-quote');
  if (review) {
    const opening = gsap.timeline({
      defaults: { duration: 1, ease: 'none' },
      scrollTrigger: {
        trigger: review, start: 'top 90%', end: 'bottom 20%',
        scrub: conditions.mobile ? true : .8, invalidateOnRefresh: true,
      },
    });
    opening
      .fromTo(review.querySelector('.review-door--left'), { x: 0, xPercent: conditions.mobile ? -12 : -18 }, { xPercent: conditions.mobile ? -16 : -24 }, 0)
      .fromTo(review.querySelector('.review-door--right'), { x: 0, xPercent: conditions.mobile ? 12 : 18 }, { xPercent: conditions.mobile ? 16 : 24 }, 0)
      .fromTo(review.querySelector('.review-vista'), { scale: 1.025 }, { scale: 1 }, 0)
      .fromTo(review.querySelector('.review-floor'), { scale: 1.008 }, { scale: 1 }, 0);
  }
});
if (import.meta.hot) import.meta.hot.dispose(() => media.revert());
