import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';

// The page scrolls away to uncover a stationary, viewport-sized footer.
// A short, masked page edge uncovers the fixed footer; GSAP adds gentle depth.
const footer = document.querySelector('.footer-stage');
const reveal = document.querySelector('.footer-reveal-space');
const nav = document.querySelector('.site-nav');
const quote = document.querySelector('.plate-quote');

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
      .fromTo(landscape, { scale: 1.045, yPercent: 3 }, { scale: 1, yPercent: 0, duration: 1 }, 0)
      .fromTo(intro, { y: 16 }, { y: 0, duration: 0.65 }, 0.1)
      .fromTo(wordmark, { yPercent: 7 }, { yPercent: 0, duration: 0.65 }, 0.2)
      .fromTo(foot, { y: 8 }, { y: 0, duration: 0.5 }, 0.35);

    if (quote) {
      gsap.fromTo(quote.querySelectorAll('.plate-img'), { yPercent: -5 }, {
        yPercent: 5,
        ease: 'none',
        scrollTrigger: {
          trigger: quote,
          start: 'top bottom',
          end: 'bottom top',
          scrub: 0.7,
          invalidateOnRefresh: true,
        },
      });
    }

    gsap.fromTo(nav, { autoAlpha: 1 }, {
      autoAlpha: 0,
      ease: 'none',
      scrollTrigger: {
        trigger: reveal,
        start: 'top 32%',
        end: 'top top',
        scrub: 0.35,
        invalidateOnRefresh: true,
      },
    });
  });
  gsap.matchMedia().add('(prefers-reduced-motion: reduce)', () => {
    ScrollTrigger.create({
      trigger: reveal,
      start: 'top top',
      onEnter: () => gsap.set(nav, { autoAlpha: 0 }),
      onLeaveBack: () => gsap.set(nav, { autoAlpha: 1 }),
    });
  });
}
