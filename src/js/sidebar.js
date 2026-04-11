// Sidebar + feature switcher. Swaps `.active` between `<button
// data-feature>` items and `<section data-view>` panels.

export function initSidebar() {
  const items = document.querySelectorAll(".feature-item");
  const views = document.querySelectorAll(".feature-view");

  items.forEach((btn) => {
    btn.addEventListener("click", () => {
      const target = btn.dataset.feature;
      items.forEach((b) => b.classList.toggle("active", b === btn));
      views.forEach((v) =>
        v.classList.toggle("active", v.dataset.view === target),
      );
    });
  });
}
