import { useLayoutEffect, useState } from 'react';

/** An element's content size, followed as it resizes; attach with `ref`. */
export function useSize<T extends HTMLElement>() {
  const [el, setEl] = useState<T | null>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  useLayoutEffect(() => {
    if (!el) return;
    setSize({ width: el.clientWidth, height: el.clientHeight });
    const ro = new ResizeObserver(() => setSize({ width: el.clientWidth, height: el.clientHeight }));
    ro.observe(el);
    return () => ro.disconnect();
  }, [el]);
  return { ref: setEl, el, ...size };
}

/** A scroll container's offsets, for windowing. */
export function useScroll() {
  const [pos, setPos] = useState({ top: 0, left: 0 });
  const onScroll = (e: { currentTarget: HTMLElement }) => setPos({ top: e.currentTarget.scrollTop, left: e.currentTarget.scrollLeft });
  return { ...pos, onScroll };
}
