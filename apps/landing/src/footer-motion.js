import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';

// The page scrolls away to uncover one viewport-sized footer.
// Independent terrain cutouts move in front of reconstructed scenery; the sky is the
// live footer surface and glyph field, not part of the raster artwork.
const footer = document.querySelector('.footer-stage');
const reveal = document.querySelector('.footer-reveal-space');
const nav = document.querySelector('.site-nav');

if (footer && reveal) {
  gsap.registerPlugin(ScrollTrigger);
  // Address-bar changes must not refresh every scene during a touch scroll.
  ScrollTrigger.config({ ignoreMobileResize: true });
  const layouts = gsap.matchMedia();
  layouts.add({ all: '(min-width: 0px)', mobile: '(max-width: 900px), (pointer: coarse)' }, ({ conditions }) => {
    const mobile = conditions.mobile;
    const root = document.documentElement;
    const viewport = window.visualViewport;
    const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
    let pullFrame = 0;
    let pull = 0;
    let targetPull = 0;
    let lastPullTime = 0;
    const renderPull = (time) => {
      const elapsed = lastPullTime ? Math.min(time - lastPullTime, 64) : 16;
      lastPullTime = time;
      pull += (targetPull - pull) * (1 - Math.exp(-elapsed / 70));
      if (Math.abs(targetPull - pull) < .001) pull = targetPull;
      footer.style.setProperty('--footer-pull', pull.toFixed(4));
      pullFrame = pull !== targetPull ? requestAnimationFrame(renderPull) : 0;
      if (!pullFrame) lastPullTime = 0;
    };
    // Use the visible height for both the fixed scene and its scroll runway.
    // Browser chrome can resize this without advancing ScrollTrigger progress.
    const syncViewportHeight = () => {
      if (mobile) {
        root.style.setProperty('--footer-viewport-height', '100svh');
        return;
      }
      if (viewport && Math.abs(viewport.scale - 1) > 0.01) return;
      const height = viewport?.height ?? window.innerHeight;
      root.style.setProperty('--footer-viewport-height', `${height}px`);
    };
    const syncFooterClip = () => {
      const bounds = footer.getBoundingClientRect();
      const scrollRoot = document.scrollingElement;
      const remaining = scrollRoot.scrollHeight - scrollRoot.clientHeight - window.scrollY;
      const covered = remaining <= 2 ? 0
        : Math.max(0, Math.min(bounds.height, reveal.getBoundingClientRect().top - bounds.top));
      footer.style.clipPath = `inset(${covered}px 0 0)`;
      // Browsers that expose elastic scroll offsets can extend the depth at
      // the end of the reveal. Others retain their native bounce unchanged.
      // Never intercept wheel/touch input or synthesize additional page scroll.
      const overscroll = Math.max(0, -remaining);
      targetPull = reducedMotion.matches || mobile ? 0 : 1 - Math.exp(-overscroll / 120);
      if (reducedMotion.matches) {
        cancelAnimationFrame(pullFrame);
        pullFrame = 0;
        lastPullTime = 0;
        pull = 0;
        footer.style.removeProperty('--footer-pull');
      } else if (!pullFrame && pull !== targetPull) {
        pullFrame = requestAnimationFrame(renderPull);
      }
    };
    let resizeFrame = 0;
    let viewportWidth = window.innerWidth;
    const syncViewport = () => {
      // Ignore toolbar/keyboard height changes; refresh on a real width change.
      if (mobile && window.innerWidth === viewportWidth) return;
      viewportWidth = window.innerWidth;
      cancelAnimationFrame(resizeFrame);
      resizeFrame = requestAnimationFrame(() => {
        syncViewportHeight();
        ScrollTrigger.refresh();
        syncFooterClip();
      });
    };
    syncViewportHeight();
    reducedMotion.addEventListener('change', syncFooterClip);
    window.addEventListener('scroll', syncFooterClip, { passive: true });
    window.addEventListener('resize', syncViewport);
    if (!mobile) viewport?.addEventListener('resize', syncViewport);
    viewport?.addEventListener('scroll', syncFooterClip, { passive: true });
    ScrollTrigger.create({
      trigger: reveal,
      start: 'top bottom',
      end: 'bottom bottom',
      onUpdate: syncFooterClip,
      onRefresh: syncFooterClip,
    });
    syncFooterClip();
    const motion = gsap.matchMedia();
    motion.add('(prefers-reduced-motion: no-preference)', () => {
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
          scrub: mobile ? true : 0.25,
          invalidateOnRefresh: true,
        },
      });

      const timeline = gsap.timeline({
        defaults: { ease: 'sine.out', duration: 1 },
        scrollTrigger: {
          trigger: reveal,
          start: 'top bottom',
          end: 'bottom bottom',
          scrub: mobile ? true : 0.45,
          invalidateOnRefresh: true,
        },
      });
      // Keep the bank visible from the start, with restrained depth separation.
      // Shared easing maintains layer order while gently settling into the scene.
      // Downward page scroll moves scenery upward. Foreground travels faster.
      // All three plates extend to the bottom and start BELOW their resting position,
      // so their full overlap covers every frame without a repeated edge strip.
      timeline
        .fromTo(far, { y: () => Math.min(mobile ? 8 : 18, landscape.clientHeight * 0.03) },
          { y: 0 }, 0)
        .fromTo(middle, { y: () => Math.min(mobile ? 16 : 38, landscape.clientHeight * 0.065) },
          { y: 0 }, 0)
        .fromTo(near, { y: () => Math.min(mobile ? 28 : 68, landscape.clientHeight * 0.115) },
          { y: 0 }, 0);

      gsap.fromTo(nav, { autoAlpha: 1 }, {
        autoAlpha: 0,
        ease: 'none',
        scrollTrigger: {
          trigger: reveal,
          start: 'top 70%',
          end: 'top 45%',
          scrub: mobile ? true : 0.35,
          invalidateOnRefresh: true,
        },
      });
    });
    motion.add('(prefers-reduced-motion: reduce)', () => {
      ScrollTrigger.create({
        trigger: reveal,
        start: 'top 45%',
        onEnter: () => gsap.set(nav, { autoAlpha: 0 }),
        onLeaveBack: () => gsap.set(nav, { autoAlpha: 1 }),
      });
    });
    return () => {
      cancelAnimationFrame(resizeFrame);
      cancelAnimationFrame(pullFrame);
      reducedMotion.removeEventListener('change', syncFooterClip);
      footer.style.removeProperty('--footer-pull');
      window.removeEventListener('scroll', syncFooterClip);
      window.removeEventListener('resize', syncViewport);
      viewport?.removeEventListener('resize', syncViewport);
      viewport?.removeEventListener('scroll', syncFooterClip);
      root.style.removeProperty('--footer-viewport-height');
      motion.revert();
      footer.style.clipPath = '';
      gsap.set(nav, { clearProps: 'opacity,visibility' });
    };
  });
  if (import.meta.hot) import.meta.hot.dispose(() => layouts.revert());
}
