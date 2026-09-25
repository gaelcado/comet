import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';

// The page scrolls away to uncover one framed, viewport-sized footer.
// Independent terrain cutouts move in front of reconstructed scenery; the sky is the
// live footer surface and glyph field, not part of the raster artwork.
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
      defaults: { ease: 'sine.out', duration: 1 },
      scrollTrigger: {
        trigger: reveal,
        start: 'top bottom',
        end: 'bottom bottom',
        scrub: 0.45,
        invalidateOnRefresh: true,
      },
    });
    // Keep the bank visible from the start, with restrained depth separation.
    // Shared easing maintains layer order while gently settling into the scene.
    // Downward page scroll moves scenery upward. Foreground travels faster.
    // All three plates extend to the bottom and start BELOW their resting position,
    // so their full overlap covers every frame without a repeated edge strip.
    timeline
      .fromTo(far, { y: () => Math.min(18, landscape.clientHeight * 0.03) },
        { y: 0 }, 0)
      .fromTo(middle, { y: () => Math.min(38, landscape.clientHeight * 0.065) },
        { y: 0 }, 0)
      .fromTo(near, { y: () => Math.min(68, landscape.clientHeight * 0.115) },
        { y: 0 }, 0);

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
