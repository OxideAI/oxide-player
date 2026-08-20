import { describe, expect, it, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { VisualizationSettings } from "./VisualizationSettings";
import type { Config, VisualizerStatus } from "../types";

const config = {
  visualizer_fft: false,
  visualizer_capture_device: "hw:Loopback,1",
  visualizer_capture_rate: 44100,
} as Config;

const status = (value: VisualizerStatus["status"]): VisualizerStatus => ({
  status: value,
  configured_enabled: value !== "disabled",
  applied_enabled: value === "running" || value === "waiting-for-capture",
  configured_source: "hw:Loopback,1",
  configured_rate: 44100,
  applied_source: "hw:Loopback,1",
  applied_rate: 44100,
  restart_required: value === "enabled-pending-restart",
  detail: value === "startup/runtime-error" ? "invalid capture source" : null,
});

const statusMock = vi.fn();
vi.mock("../api", () => ({
  api: {
    visualizerStatus: (...args: unknown[]) => statusMock(...args),
  },
}));

describe("VisualizationSettings", () => {
  beforeEach(() => {
    statusMock.mockResolvedValue(status("disabled"));
  });

  it.each([
    ["disabled", "Disabled"],
    ["enabled-pending-restart", "Enabled · restart pending"],
    ["running", "Running"],
    ["waiting-for-capture", "Waiting for capture"],
    ["startup/runtime-error", "Capture error"],
  ] as const)("renders %s distinctly", async (state, copy) => {
    statusMock.mockResolvedValue(status(state));
    render(
      <VisualizationSettings
        config={{ ...config, visualizer_fft: state !== "disabled" }}
        onSave={vi.fn()}
      />,
    );
    expect(
      (await screen.findByTestId("visualizer-status")).textContent,
    ).toContain(copy);
    if (state === "waiting-for-capture")
      expect(screen.getByText(/non-terminal/i)).toBeTruthy();
    if (state === "startup/runtime-error")
      expect(screen.getByText(/invalid capture source/i)).toBeTruthy();
  });

  it("saves enablement through the supplied config boundary and keeps style tuning separate", async () => {
    const onSave = vi
      .fn()
      .mockResolvedValue({ ...config, visualizer_fft: true });
    render(<VisualizationSettings config={config} onSave={onSave} />);
    fireEvent.click(
      screen.getByRole("checkbox", { name: /Enable visualization/i }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: /Save visualization/i }),
    );
    await waitFor(() => expect(onSave).toHaveBeenCalledWith(true));
    expect(
      (
        screen.getByRole("checkbox", {
          name: /Enable visualization/i,
        }) as HTMLInputElement
      ).checked,
    ).toBe(true);
  });

  it("keeps a failed draft and cancel restores the saved value", async () => {
    const onSave = vi.fn().mockRejectedValue(new Error("disk full"));
    render(<VisualizationSettings config={config} onSave={onSave} />);
    fireEvent.click(
      screen.getByRole("checkbox", { name: /Enable visualization/i }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: /Save visualization/i }),
    );
    expect(await screen.findByText(/Save failed: disk full/i)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(
      (
        screen.getByRole("checkbox", {
          name: /Enable visualization/i,
        }) as HTMLInputElement
      ).checked,
    ).toBe(false);
  });
});
