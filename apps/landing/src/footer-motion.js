import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';

// The page scrolls away to uncover one framed, viewport-sized footer.
// Four source-aligned cutouts move independently as the footer is uncovered.
const footer = document.querySelector('.footer-stage');
const reveal = document.querySelector('.footer-reveal-space');
const nav = document.querySelector('.site-nav');
const quote = document.querySelector('.plate-quote');

if (footer && reveal) {
  gsap.registerPlugin(ScrollTrigger);
  gsap.matchMedia().add('(prefers-reduced-motion: no-preference)', () => {
    const frame = footer.querySelector('.footer-frame');
    const landscape = footer.querySelector('.footer-landscape');
    const sun = footer.querySelector('.footer-plane--sun');
    const far = footer.querySelector('.footer-plane--far');
    const middle = footer.querySelector('.footer-plane--middle');
    const near = footer.querySelector('.footer-plane--near');

    gsap.fromTo(frame, { opacity: 0 }, {
      opacity: 1,
      ease: 'none',
      scrollTrigger: {
        trigger: reveal,
        start: 'top bottom',
        end: 'top 45%',
        scrub: 0.25,
        invalidateOnRefresh: true,
      },
    });

    const timeline = gsap.timeline({
      defaults: { ease: 'power2.out' },
      scrollTrigger: {
        trigger: reveal,
        start: 'top bottom',
        end: 'bottom bottom',
        scrub: 0.18,
        invalidateOnRefresh: true,
      },
    });
    timeline
      .fromTo(sun, { y: () => landscape.clientHeight * 0.025 }, { y: 0, ease: 'none', duration: 1 }, 0)
      .fromTo(far, { y: () => landscape.clientHeight * 0.04 }, { y: 0, ease: 'none', duration: 1 }, 0)
      .fromTo(middle, { y: () => landscape.clientHeight * 0.07 }, { y: 0, ease: 'none', duration: 1 }, 0)
      .fromTo(near, { y: () => landscape.clientHeight * 0.10 }, { y: 0, ease: 'none', duration: 1 }, 0);

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
        start: 'top 70%',
        end: 'top 45%',
        scrub: 0.35,
        invalidateOnRefresh: true,
      },
    });
  });
  gsap.matchMedia().add('(prefers-reduced-motion: reduce)', () => {
    ScrollTrigger.create({
      trigger: reveal,
      start: 'top 45%',
      onEnter: () => gsap.set(nav, { autoAlpha: 0 }),
      onLeaveBack: () => gsap.set(nav, { autoAlpha: 1 }),
    });
  });
}
