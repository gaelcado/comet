import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';

// Static HTML is the complete scene. Enhance only the active SVG so mobile and
// desktop keep independent geometry, and media-query changes restore defaults.
const scene = document.querySelector('.collaboration');
if (scene) {
  gsap.registerPlugin(ScrollTrigger);
  const media = gsap.matchMedia();
  media.add({ mobile: '(max-width: 700px)', desktop: '(min-width: 701px)', reduce: '(prefers-reduced-motion: reduce)' }, ({ conditions }) => {
    if (conditions.reduce) return;
    const stage = scene.querySelector('.network-stage');
    const svg = stage.querySelector(conditions.mobile ? '.network-paths--mobile' : '.network-paths--desktop');
    const timeline = gsap.timeline({ scrollTrigger: {
      trigger: stage, start: 'top 85%', end: 'center 45%', scrub: 0.65,
    } });
    // Only the foreground strokes animate. Static tracks retain the
    // full topology; no SVG filters, travelling glow layers, chip transforms,
    // or moving chips are needed while scrolling.
    for (const side of ['agents', 'devices']) {
      const cores = [...svg.querySelectorAll(`.network-draw[data-connection="${side}"]`)];
      const midpoint = (cores.length - 1) / 2;
      cores.forEach((core, index) => {
        const length = core.getTotalLength();
        const delay = Math.abs(index - midpoint) / midpoint * 0.24;
        timeline.fromTo(core, {
          strokeDasharray: length,
          strokeDashoffset: length,
        }, { strokeDashoffset: 0, duration: 0.76, ease: 'none' }, delay);
      });
    }
  });
  if (import.meta.hot) import.meta.hot.dispose(() => media.revert());
}
