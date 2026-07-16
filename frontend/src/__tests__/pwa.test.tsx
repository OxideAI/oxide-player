import { describe, it, expect, beforeEach, vi } from 'vitest'
import { render, screen, fireEvent, act } from '@testing-library/react'
import { InstallPrompt } from '../components/InstallPrompt'
import { OfflineBanner } from '../components/OfflineBanner'

const DISMISS_KEY = 'oxide:pwa-install-dismissed'

function emit(name: string) {
  act(() => {
    window.dispatchEvent(new Event(name))
  })
}

function fireBeforeInstall() {
  const evt = new Event('beforeinstallprompt') as Event & {
    prompt: () => Promise<void>
    userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }>
  }
  evt.preventDefault = () => {}
  evt.prompt = vi.fn(async () => {})
  evt.userChoice = Promise.resolve({ outcome: 'accepted' })
  act(() => {
    window.dispatchEvent(evt)
  })
  return evt
}

beforeEach(() => {
  localStorage.clear()
  vi.restoreAllMocks()
})

describe('InstallPrompt', () => {
  it('does not render without a beforeinstallprompt event', () => {
    render(<InstallPrompt />)
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('shows after beforeinstallprompt and can be dismissed (persisted)', () => {
    render(<InstallPrompt />)
    fireBeforeInstall()
    const dialog = screen.getByRole('dialog')
    expect(dialog).toBeTruthy()
    fireEvent.click(screen.getByLabelText('Dismiss'))
    expect(screen.queryByRole('dialog')).toBeNull()
    expect(localStorage.getItem(DISMISS_KEY)).toBe('1')
  })

  it('stays hidden when previously dismissed', () => {
    localStorage.setItem(DISMISS_KEY, '1')
    render(<InstallPrompt />)
    fireBeforeInstall()
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('calls prompt() and hides on install', async () => {
    render(<InstallPrompt />)
    const evt = fireBeforeInstall()
    fireEvent.click(screen.getByText('Install'))
    await new Promise((r) => setTimeout(r, 0))
    expect(evt.prompt).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('does not call prompt() when dismissed', () => {
    render(<InstallPrompt />)
    const evt = fireBeforeInstall()
    fireEvent.click(screen.getByLabelText('Dismiss'))
    expect(evt.prompt).not.toHaveBeenCalled()
    expect(localStorage.getItem(DISMISS_KEY)).toBe('1')
  })

  it('invokes prompt only once even if Install is clicked twice', async () => {
    render(<InstallPrompt />)
    const evt = fireBeforeInstall()
    const btn = screen.getByText('Install')
    fireEvent.click(btn)
    fireEvent.click(btn)
    await new Promise((r) => setTimeout(r, 0))
    expect(evt.prompt).toHaveBeenCalledTimes(1)
  })

  it('clears the dismissed flag on appinstalled', () => {
    localStorage.setItem(DISMISS_KEY, '1')
    render(<InstallPrompt />)
    act(() => {
      window.dispatchEvent(new Event('appinstalled'))
    })
    expect(localStorage.getItem(DISMISS_KEY)).toBeNull()
  })
})

describe('OfflineBanner', () => {
  it('renders when offline', () => {
    vi.spyOn(navigator, 'onLine', 'get').mockReturnValue(false)
    render(<OfflineBanner />)
    expect(screen.getByRole('status')).toBeTruthy()
  })

  it('does not render when online', () => {
    vi.spyOn(navigator, 'onLine', 'get').mockReturnValue(true)
    render(<OfflineBanner />)
    expect(screen.queryByRole('status')).toBeNull()
  })

  it('appears when the offline event fires', () => {
    vi.spyOn(navigator, 'onLine', 'get').mockReturnValue(true)
    render(<OfflineBanner />)
    expect(screen.queryByRole('status')).toBeNull()
    emit('offline')
    expect(screen.getByRole('status')).toBeTruthy()
    emit('online')
    expect(screen.queryByRole('status')).toBeNull()
  })
})
