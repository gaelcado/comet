// The final viewport stays fixed briefly while the mountain engraving settles.
// ScrollTrigger owns the pin and restores the normal document flow afterward.
(() => {
  if (!window.gsap || !window.ScrollTrigger) return;

  const footer = document.querySelector('.footer-stage');
  const landscape = footer?.querySelector('.footer-landscape');
  const intro = footer?.querySelector('.footer-intro');
  const wordmark = footer?.querySelector('.footer-wordmark');
  const foot = footer?.querySelector('.foot');
  if (!footer || !landscape || !intro || !wordmark || !foot) return;

  gsap.registerPlugin(ScrollTrigger);
  const media = gsap.matchMedia();
  media.add('(min-height: 640px) and (prefers-reduced-motion: no-preference)', () => {
    const timeline = gsap.timeline({
      defaults: { ease: 'none' },
      scrollTrigger: {
        trigger: footer,
        start: 'top top',
        end: () => `+=${Math.min(Math.round(window.innerHeight * 0.65), 560)}`,
        pin: true,
        pinSpacing: true,
        anticipatePin: 1,
        scrub: 0.7,
        invalidateOnRefresh: true,
      },
    });
    timeline
      .fromTo(landscape, { yPercent: 7, scale: 1.07 }, { yPercent: 0, scale: 1 }, 0)
      .fromTo(intro, { y: 28, opacity: 0.68 }, { y: 0, opacity: 1 }, 0)
      .fromTo(wordmark, { yPercent: 23, opacity: 0.35 }, { yPercent: 0, opacity: 1 }, 0.08)
      .fromTo(foot, { y: 12, opacity: 0.45 }, { y: 0, opacity: 1 }, 0.28);
  });
})();
