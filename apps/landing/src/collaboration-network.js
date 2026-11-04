import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';

// Static HTML is the complete scene. Desktop draws its converging paths on
// scroll; narrow screens use a stacked CSS layout with no scroll-linked motion.
const scene = document.querySelector('.collaboration');
if (scene) {
  gsap.registerPlugin(ScrollTrigger);
  const media = gsap.matchMedia();
  media.add('(min-width: 701px) and (prefers-reduced-motion: no-preference)', () => {
    const stage = scene.querySelector('.network-stage');
    const svg = stage.querySelector('.network-paths--desktop');
    // Each stream feeds the hub's light ring as it lands: a line contributes
    // over the last quarter of its draw, so the ring fills as arrivals stack.
    const arrivals = [];
    const charge = () => {
      const total = arrivals.reduce((sum, tween) => sum + gsap.utils.clamp(0, 1, (tween.progress() - 0.75) / 0.25), 0);
      stage.style.setProperty('--network-charge', (total / arrivals.length).toFixed(3));
    };
    const timeline = gsap.timeline({ onUpdate: charge, scrollTrigger: {
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
        const tween = gsap.fromTo(core, {
          strokeDasharray: length,
          strokeDashoffset: length,
        }, { strokeDashoffset: 0, duration: 0.76, ease: 'none' });
        arrivals.push(tween);
        timeline.add(tween, delay);
      });
    }
    charge();
    return () => stage.style.removeProperty('--network-charge');
  });
  if (import.meta.hot) import.meta.hot.dispose(() => media.revert());
}
