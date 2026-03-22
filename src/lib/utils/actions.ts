export function focus(node: HTMLElement, enabled = true) {
  if (enabled) node.focus();
  return {};
}

export function sticky(node: HTMLElement, { top = 0 } = {}) {
  let scroller: Element | null, section: HTMLElement | null;
  let nodeTop: number, sectionTop: number;

  function findScroller() {
    let el: HTMLElement | null = node.parentElement;
    while (el) {
      const ov = getComputedStyle(el).overflowY;
      if (ov === 'auto' || ov === 'scroll') return el;
      el = el.parentElement;
    }
    return null;
  }

  function computeOffsets() {
    const saved = node.style.transform;
    node.style.transform = 'none';
    const scrollerRect = scroller!.getBoundingClientRect();
    const nr = node.getBoundingClientRect();
    const sr = section!.getBoundingClientRect();
    node.style.transform = saved;
    nodeTop = nr.top - scrollerRect.top + (scroller as HTMLElement).scrollTop;
    sectionTop = sr.top - scrollerRect.top + (scroller as HTMLElement).scrollTop;
  }

  function onScroll() {
    const scrollTop = (scroller as HTMLElement).scrollTop;
    const sectionHeight = section!.offsetHeight;
    const nodeHeight = node.offsetHeight;
    if (scrollTop + top > nodeTop) {
      const shift = Math.max(0, Math.min(
        scrollTop + top - nodeTop,
        sectionTop + sectionHeight - nodeTop - nodeHeight
      ));
      node.style.transform = `translateY(${shift}px)`;
    } else {
      node.style.transform = '';
    }
  }

  requestAnimationFrame(() => {
    scroller = findScroller();
    section = node.parentElement;
    if (!scroller) return;
    computeOffsets();
    scroller.addEventListener('scroll', onScroll, { passive: true });
    onScroll();
  });

  return {
    destroy() {
      if (scroller) scroller.removeEventListener('scroll', onScroll);
    }
  };
}
