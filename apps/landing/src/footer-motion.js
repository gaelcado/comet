import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';

// The page scrolls away to uncover a stationary footer below the CTA prelude.
// Four source-aligned cutouts move independently as the footer is uncovered.
const footer = document.querySelector('.footer-stage');
const reveal = document.querySelector('.footer-reveal-space');
const prelude = document.querySelector('.footer-intro-section');
const nav = document.querySelector('.site-nav');
const quote = document.querySelector('.plate-quote');

if (footer && reveal) {
  gsap.registerPlugin(ScrollTrigger);
  gsap.matchMedia().add('(prefers-reduced-motion: no-preference)', () => {
    const landscape = footer.querySelector('.footer-landscape');
    const sun = footer.querySelector('.footer-plane--sun');
    const far = footer.querySelector('.footer-plane--far');
    const middle = footer.querySelector('.footer-plane--middle');
    const near = footer.querySelector('.footer-plane--near');
    const wordmark = footer.querySelector('.footer-wordmark');
    const foot = footer.querySelector('.foot');

    const timeline = gsap.timeline({
      defaults: { ease: 'power2.out' },
      scrollTrigger: {
        trigger: reveal,
        start: 'top bottom',
        end: 'bottom bottom',
        scrub: 0.35,
        invalidateOnRefresh: true,
      },
    });
    timeline
      .fromTo(sun, { y: () => landscape.clientHeight * 0.01 }, { y: 0, ease: 'none', duration: 1 }, 0)
      .fromTo(far, { y: () => landscape.clientHeight * 0.015 }, { y: 0, ease: 'none', duration: 1 }, 0)
      .fromTo(middle, { y: () => landscape.clientHeight * 0.03 }, { y: 0, ease: 'none', duration: 1 }, 0)
      .fromTo(near, { y: () => landscape.clientHeight * 0.045 }, { y: 0, ease: 'none', duration: 1 }, 0)
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
        trigger: prelude,
        start: 'top 75%',
        end: 'top top',
        scrub: 0.35,
        invalidateOnRefresh: true,
      },
    });
  });
  gsap.matchMedia().add('(prefers-reduced-motion: reduce)', () => {
    ScrollTrigger.create({
      trigger: prelude,
      start: 'top top',
      onEnter: () => gsap.set(nav, { autoAlpha: 0 }),
      onLeaveBack: () => gsap.set(nav, { autoAlpha: 1 }),
    });
  });
}
