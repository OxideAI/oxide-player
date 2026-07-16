export const revealClass = 'ox-reveal'

if (typeof document !== 'undefined') {
  const style = document.createElement('style')
  style.dataset.ox = 'anim'
  style.textContent = `
.ox-reveal {
  opacity: 0;
  transform: translate3d(0, 28px, 0);
  filter: blur(8px);
  transition: opacity 0.9s var(--ease-out, ease), transform 0.9s var(--ease-out, ease),
    filter 0.9s var(--ease-out, ease);
  will-change: transform, opacity, filter;
}
  .ox-reveal.is-in {
    opacity: 1;
    transform: translate3d(0, 0, 0);
    filter: blur(0);
    will-change: auto;
  }
  @media (prefers-reduced-motion: reduce) {
    .ox-reveal { opacity: 1; transform: none; filter: none; transition: none; }
  }
`
  document.head.appendChild(style)
}

const io: IntersectionObserver | null =
  typeof IntersectionObserver !== 'undefined'
    ? new IntersectionObserver(
        (entries) => {
          for (const e of entries) {
            if (e.isIntersecting) {
              e.target.classList.add('is-in')
              io?.unobserve(e.target)
            }
          }
        },
        { rootMargin: '0px 0px -8% 0px', threshold: 0.08 },
      )
    : null

export function observeReveal(el: HTMLElement | null, delay = 0): () => void {
  if (!el) return () => {}
  if (delay) el.style.transitionDelay = `${delay}ms`
  if (!io) {
    el.classList.add('is-in')
    return () => {}
  }
  io.observe(el)
  return () => io.unobserve(el)
}
