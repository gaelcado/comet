const footer = document.querySelector('.footer-stage');
const landscape = footer?.querySelector('.footer-landscape');
const buttons = [...(footer?.querySelectorAll('button[data-footer-scene]') ?? [])];
const scenes = ['terraces', 'colonnade', 'bridge'];
const plates = { far: 'far', middle: 'monument', near: 'foreground' };

function selectScene(scene) {
  if (!landscape || !scenes.includes(scene)) return;
  footer.dataset.footerScene = scene;
  for (const [plane, asset] of Object.entries(plates)) {
    for (const theme of ['dark', 'light']) {
      landscape.style.setProperty(`--monument-${plane}-${theme}`,
        `url('/assets/art/footer-monuments/${scene}-${theme}-${asset}.webp')`);
    }
  }
  buttons.forEach(button => button.setAttribute('aria-pressed', String(button.dataset.footerScene === scene)));
}

const onClick = event => selectScene(event.currentTarget.dataset.footerScene);
buttons.forEach(button => button.addEventListener('click', onClick));
selectScene('terraces');

if (import.meta.hot) import.meta.hot.dispose(() => {
  buttons.forEach(button => button.removeEventListener('click', onClick));
});
