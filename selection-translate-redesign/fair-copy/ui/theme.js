// Theme switch — light (paper, reference) and dark (the desk after dusk).
// Applies data-theme on <html>; #theme=dark deep-links for review captures;
// window.setTheme is wired to the manager's Theme select and every .themebtn.
(() => {
  const apply = (t) => {
    const theme = t === 'dark' ? 'dark' : 'light';
    document.documentElement.dataset.theme = theme;
    const sel = document.getElementById('uitheme');
    if (sel) sel.value = theme === 'dark' ? 'Dark' : 'Light';
    document.dispatchEvent(new CustomEvent('themechange', { detail: theme }));
  };
  window.setTheme = apply;

  document.addEventListener('themechange', () => {
    const dark = document.documentElement.dataset.theme === 'dark';
    document.querySelectorAll('.themebtn').forEach(b => {
      b.textContent = dark ? 'Light theme' : 'Dark theme';
    });
  });
  document.querySelectorAll('.themebtn').forEach(b => {
    b.addEventListener('click', () =>
      apply(document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark'));
  });

  const h = (location.hash.match(/theme=(\w+)/) || [])[1];
  apply(h || 'light');
})();
