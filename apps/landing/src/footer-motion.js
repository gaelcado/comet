import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';

// The page scrolls away to uncover one framed, viewport-sized footer.
// Three soft-masked depth bands move independently over the source-aligned base.
const footer = document.querySelector('.footer-stage');
const reveal = document.querySelector('.footer-reveal-space');
const nav = document.querySelector('.site-nav');
const quote = document.querySelector('.plate-quote');

if (footer && reveal) {
  gsap.registerPlugin(ScrollTrigger);
  // Clip the fixed frame to the portion uncovered by the scrolling document.
  // This also keeps its side border from leaking beside the scrollbar at zoom.
  const syncFooterClip = () => {
    const bounds = footer.getBoundingClientRect();
    const covered = Math.max(0, Math.min(bounds.height, reveal.getBoundingClientRect().top - bounds.top));
    footer.style.clipPath = `inset(${covered}px 0 0)`;
  };
  ScrollTrigger.create({
    trigger: reveal,
    start: 'top bottom',
    end: 'bottom bottom',
    onUpdate: syncFooterClip,
    onRefresh: syncFooterClip,
  });
  syncFooterClip();
  gsap.matchMedia().add('(prefers-reduced-motion: no-preference)', () => {
    const frame = footer.querySelector('.footer-frame');
    const landscape = footer.querySelector('.footer-landscape');
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
      .fromTo(far, { y: () => landscape.clientHeight * 0.012 }, { y: 0, ease: 'none', duration: 1 }, 0)
      .fromTo(middle, { y: () => landscape.clientHeight * 0.027 }, { y: 0, ease: 'none', duration: 1 }, 0)
      .fromTo(near, { y: () => landscape.clientHeight * 0.045 }, { y: 0, ease: 'none', duration: 1 }, 0);

    if (quote) {
      gsap.fromTo(quote.querySelectorAll('.plate-img'), { yPercent: -2 }, {
        yPercent: 2,
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
