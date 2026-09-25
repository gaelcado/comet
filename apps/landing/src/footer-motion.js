import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';

// The page scrolls away to uncover a stationary, viewport-sized footer.
// GSAP adds a small fade and parallax while the last page-height clears it.
const footer = document.querySelector('.footer-stage');
const reveal = document.querySelector('.footer-reveal-space');

if (footer && reveal) {
  gsap.registerPlugin(ScrollTrigger);
  gsap.matchMedia().add('(prefers-reduced-motion: no-preference)', () => {
    const landscape = footer.querySelector('.footer-landscape');
    const intro = footer.querySelector('.footer-intro');
    const wordmark = footer.querySelector('.footer-wordmark');
    const foot = footer.querySelector('.foot');

    const timeline = gsap.timeline({
      defaults: { ease: 'power2.out' },
      scrollTrigger: {
        trigger: reveal,
        start: 'top bottom',
        end: 'bottom bottom',
        scrub: 0.6,
        invalidateOnRefresh: true,
      },
    });
    timeline
      .fromTo(footer, { opacity: 0.6 }, { opacity: 1, duration: 0.5 }, 0)
      .fromTo(landscape, { scale: 1.07, yPercent: 5 }, { scale: 1, yPercent: 0, duration: 0.9 }, 0)
      .fromTo(intro, { y: 30, opacity: 0.4 }, { y: 0, opacity: 1, duration: 0.55 }, 0.12)
      .fromTo(wordmark, { yPercent: 16, opacity: 0.2 }, { yPercent: 0, opacity: 1, duration: 0.55 }, 0.25)
      .fromTo(foot, { y: 12, opacity: 0.25 }, { y: 0, opacity: 1, duration: 0.4 }, 0.4);
  });
}
