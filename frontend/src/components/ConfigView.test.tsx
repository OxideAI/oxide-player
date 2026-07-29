import { describe, it, expect, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import { ConfigView } from './ConfigView'

vi.mock('../api', () => ({
  api: {
    getConfig: vi.fn().mockResolvedValue({
      library_dirs: [],
      mpd_host: '127.0.0.1',
      mpd_port: 6600,
      listen: '0.0.0.0:8000',
      camilladsp_config_path: '',
      camilladsp_ws_url: null,
      data_dir: '/tmp',
      static_dir: '/tmp',
    }),
    version: vi.fn().mockResolvedValue({ backend: '0.1.0', frontend: '0.1.0' }),
  },
}))

vi.mock('./DevicesView', () => ({ DevicesView: () => null }))
vi.mock('./DspView', () => ({ DspView: () => null }))

describe('ConfigView versions', () => {
  it('shows backend and frontend versions at the bottom of settings', async () => {
    render(<ConfigView />)
    await waitFor(() => {
      expect(screen.getByText(/App versions/i)).toBeTruthy()
    })
    expect(screen.getByText(/Backend 0\.1\.0/i)).toBeTruthy()
    expect(screen.getByText(/Frontend 0\.1\.0/i)).toBeTruthy()
  })
})
