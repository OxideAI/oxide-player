import { describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api } from "../api";
import { ConfigView } from "./ConfigView";

vi.mock("../api", () => ({
  api: {
    getConfig: vi.fn().mockResolvedValue({
      library_dirs: [],
      mpd_host: "127.0.0.1",
      mpd_port: 6600,
      bluetooth_reconnect_on_startup: true,
      listen: "0.0.0.0:8000",
      camilladsp_config_path: "",
      camilladsp_ws_url: null,
      data_dir: "/tmp",
      static_dir: "/tmp",
    }),
    disableDevice: vi.fn().mockResolvedValue(undefined),
    enableDevice: vi.fn().mockResolvedValue(undefined),
    version: vi.fn().mockResolvedValue({ version: "0.1.0" }),
    devices: vi.fn().mockResolvedValue([
      {
        id: 1,
        name: "USB DAC",
        enabled: true,
        role: "playback",
        selectable: true,
        selection_key: "alsa:USB DAC",
        configured: true,
        available: true,
        connected: null,
        active: true,
        dsp_supported: true,
        dsp_enabled: false,
      },
    ]),
  },
}));

vi.mock("./DevicesView", () => ({ DevicesView: () => null }));

vi.mock("./DspView", () => ({ DspView: () => null }));

describe("ConfigView version", () => {
  it("shows the app version at the bottom of settings", async () => {
    render(<ConfigView />);
    await waitFor(() => {
      expect(screen.getByText(/Version 0\.1\.0/i)).toBeTruthy();
      const checkbox = screen.getByRole("checkbox", {
        name: /Reconnect paired speakers on startup/i,
      }) as HTMLInputElement;
      expect(checkbox.checked).toBe(true);
    });
  });
  it("shows MPD route status without claiming audible DAC output while config loads", async () => {
    render(
      <ConfigView
        status={{
          state: "playing",
          volume: 70,
          elapsed: 4,
          duration: 120,
          outputs: [{ id: 1, name: "USB DAC", enabled: true }],
          error: null,
          current_song: null,
          random: false,
        }}
        statusConnected
        statusLastUpdatedAt={Date.now()}
      />,
    );
    expect(await screen.findByText("Playing in MPD")).toBeTruthy();
    expect(
      screen.getByText(/cannot be verified here/i),
    ).toBeTruthy();
    expect(screen.getAllByText("USB DAC").length).toBeGreaterThan(0);
    expect(
      await screen.findByRole("button", {
        name: "Reconnect USB / DAC output",
      }),
    ).toBeTruthy();
  });

  it("reconnects an enabled USB output by toggling its MPD output", async () => {
    render(
      <ConfigView
        status={{
          state: "playing",
          volume: 70,
          elapsed: 4,
          duration: 120,
          outputs: [{ id: 1, name: "USB DAC", enabled: true }],
          error: null,
          current_song: null,
          random: false,
        }}
        statusConnected
        statusLastUpdatedAt={Date.now()}
      />,
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Reconnect USB / DAC output" }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Reconnect output" }),
    );
    await waitFor(() => {
      expect(api.disableDevice).toHaveBeenCalledWith(1);
      expect(api.enableDevice).toHaveBeenCalledWith(1);
      expect(
        screen.getByText(/Check whether sound has returned/i),
      ).toBeTruthy();
    });
  });
});
