import './styleguide.css';

const themeButton = document.querySelector('.theme-toggle');

function syncThemeButton() {
  const next = document.documentElement.dataset.theme === 'light' ? 'dark' : 'light';
  themeButton.setAttribute('aria-label', `Switch to ${next} mode`);
  themeButton.title = `Switch to ${next} mode`;
}

themeButton.addEventListener('click', () => {
  const next = document.documentElement.dataset.theme === 'light' ? 'dark' : 'light';
  document.documentElement.dataset.theme = next;
  try {
    localStorage.setItem('zeron-landing-theme', next);
  } catch {}
  syncThemeButton();
});
syncThemeButton();
