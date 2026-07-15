import { useEffect, useRef } from 'react'
import type { CSSProperties, ElementType, ReactNode } from 'react'
import { observeReveal } from '../animations'

interface Props {
  children: ReactNode
  as?: ElementType
  className?: string
  delay?: number
  style?: CSSProperties
  [key: string]: unknown
}

/** Wraps content in a viewport-entry reveal (fade + blur-up + rise). */
export function Reveal({ children, as: Tag = 'div', className, delay = 0, ...rest }: Props) {
  const ref = useRef<HTMLElement>(null)
  useEffect(() => {
    const el = ref.current
    if (!el) return
    el.classList.add('ox-reveal')
    return observeReveal(el, delay)
  }, [delay])
  return (
    <Tag ref={ref as never} className={className} {...rest}>
      {children}
    </Tag>
  )
}
