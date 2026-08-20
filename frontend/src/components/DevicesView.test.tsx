import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import type { api as apiClient } from "../api";
import { ApiError } from "../api";
import type {
  BtDevice,
  DeviceConfig,
  InputStatusResponse,
  OutputDevice,
  UsbAudioDevice,
} from "../types";
const listDevices = vi.fn<() => Promise<OutputDevice[]>>();
const listConfigs = vi.fn<() => Promise<DeviceConfig[]>>();
const listUsbDevices = vi.fn<() => Promise<UsbAudioDevice[]>>();
const listBluetoothDevices = vi.fn<() => Promise<BtDevice[]>>();
const inputStatus = vi.fn<() => Promise<InputStatusResponse>>();
const wakeConnect = vi.fn<(address: string) => Promise<unknown>>();
const pair = vi.fn<(address: string) => Promise<unknown>>();
const disconnect = vi.fn<(address: string) => Promise<unknown>>();
const enableDeviceDsp = vi.fn<(id: number) => Promise<unknown>>();
const disableDeviceDsp = vi.fn<(id: number) => Promise<unknown>>();
const restartMpd = vi.fn<() => Promise<unknown>>();

vi.mock("../api", async (importOriginal) => {
  const actual = await importOriginal<{
    api: typeof apiClient;
    ApiError: typeof ApiError;
  }>();
  return {
    ...actual,
    api: {
      ...actual.api,
      devices: () => listDevices(),
      deviceConfigs: () => listConfigs(),
      usbDevices: () => listUsbDevices(),
      btDevices: () => listBluetoothDevices(),
      btInputStatus: () => inputStatus(),
      btWakeConnect: (address: string) => wakeConnect(address),
      btPair: (address: string) => pair(address),
      btDisconnect: (address: string) => disconnect(address),
      disableDeviceDsp: (id: number) => disableDeviceDsp(id),
      enableDeviceDsp: (id: number) => enableDeviceDsp(id),
      restartMpd: () => restartMpd(),
    },
  };
});

import { DevicesView } from "./DevicesView";

function makeDevice(over: Partial<BtDevice> = {}): BtDevice {
  return {
    address: "AA:BB:CC:DD:EE:FF",
    name: "Living Room Speaker",
    alias: null,
    class: null,
    icon: null,
    rssi: -42,
    connected: false,
    paired: true,
    trusted: false,
    ...over,
  };
}
function makeBluetoothConfig(address = "AA:BB:CC:DD:EE:FF"): DeviceConfig {
  return {
    name: "Living Room Speaker",
    output_type: "bluealsa",
    device: `bluealsa:DEV=${address},PROFILE=a2dp`,
    format: "48000:16:2",
    mixer_type: null,
    mixer_device: null,
    dop: false,
    restart_pending: false,
  };
}

function makeOutput(over: Partial<OutputDevice> = {}): OutputDevice {
  return {
    id: 1,
    name: "USB DAC",
    enabled: true,
    dsp_supported: true,
    dsp_enabled: false,
    ...over,
  };
}
describe("DevicesView output controls", () => {
  beforeEach(() => {
    listDevices.mockResolvedValue([]);
    listConfigs.mockResolvedValue([]);
    listUsbDevices.mockResolvedValue([]);
    listBluetoothDevices.mockResolvedValue([]);
    inputStatus.mockResolvedValue({ enabled: false, streaming: false });
    wakeConnect.mockResolvedValue({});
    pair.mockResolvedValue({});
    disconnect.mockResolvedValue({});
    enableDeviceDsp.mockResolvedValue({});
    disableDeviceDsp.mockResolvedValue({});
    restartMpd.mockResolvedValue({});
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("always offers a manual MPD restart control", async () => {
    const { getByRole } = render(<DevicesView />);
    const restart = getByRole("button", { name: "Restart MPD" });

    fireEvent.click(restart);
    await waitFor(() => expect(restartMpd).toHaveBeenCalledTimes(1));
  });
  it("enables DSP for a supported output and disables unsupported controls", async () => {
    listDevices.mockResolvedValue([
      makeOutput(),
      makeOutput({
        id: 2,
        name: "Pipe monitor",
        dsp_supported: false,
        dsp_reason: "only ALSA outputs support CamillaDSP",
      }),
    ]);

    const { getByRole } = render(<DevicesView />);
    const enable = await waitFor(() =>
      getByRole("button", { name: "Enable DSP for USB DAC" }),
    );
    const unsupported = getByRole("button", {
      name: "Enable DSP for Pipe monitor",
    });

    expect(enable).not.toHaveProperty("disabled", true);
    expect(unsupported).toHaveProperty("disabled", true);
    fireEvent.click(enable);
    await waitFor(() => expect(enableDeviceDsp).toHaveBeenCalledWith(1));
  });

  it("offers disabling DSP when the output is routed through CamillaDSP", async () => {
    listDevices.mockResolvedValue([
      makeOutput({ dsp_enabled: true, enabled: false }),
    ]);

    const { getByRole } = render(<DevicesView />);
    const disable = await waitFor(() =>
      getByRole("button", { name: "Disable DSP for USB DAC" }),
    );

    fireEvent.click(disable);
    await waitFor(() => expect(disableDeviceDsp).toHaveBeenCalledWith(1));
  });
  it("scans USB audio hardware and opens its device details", async () => {
    listUsbDevices.mockResolvedValue([
      {
        id: "alsa:2:0",
        name: "Topping USB DAC",
        card: 2,
        device: 0,
        alsa_device: "hw:2,0",
      },
    ]);

    const { getByRole, getByText, getByDisplayValue } = render(<DevicesView />);
    fireEvent.click(getByRole("button", { name: "Scan USB audio devices" }));
    await waitFor(() => expect(getByText("Topping USB DAC")).toBeTruthy());
    fireEvent.click(getByRole("button", { name: "Configure device" }));
    expect(getByDisplayValue("Topping USB DAC")).toBeTruthy();
    expect(getByDisplayValue("hw:2,0")).toBeTruthy();
  });
});

describe("DevicesView Bluetooth actions", () => {
  beforeEach(() => {
    listDevices.mockResolvedValue([]);
    listConfigs.mockResolvedValue([]);
    listBluetoothDevices.mockResolvedValue([]);
    inputStatus.mockResolvedValue({ enabled: false, streaming: false });
    wakeConnect.mockResolvedValue({});
    pair.mockResolvedValue({});
    disconnect.mockResolvedValue({});
    enableDeviceDsp.mockResolvedValue({});
    disableDeviceDsp.mockResolvedValue({});
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("shows Connect for a paired disconnected device and wakes it on click", async () => {
    listBluetoothDevices.mockResolvedValue([makeDevice()]);

    const { getByRole, queryByRole } = render(<DevicesView />);
    await waitFor(() =>
      expect(getByRole("button", { name: "Connect" })).toBeTruthy(),
    );

    expect(queryByRole("button", { name: "Wake & Connect" })).toBeNull();
    expect(queryByRole("button", { name: "Disconnect" })).toBeNull();

    fireEvent.click(getByRole("button", { name: "Connect" }));
    await waitFor(() =>
      expect(wakeConnect).toHaveBeenCalledWith("AA:BB:CC:DD:EE:FF"),
    );
  });
  it("keeps Connect available for a configured paired speaker after disconnect", async () => {
    listBluetoothDevices.mockResolvedValue([makeDevice()]);
    listConfigs.mockResolvedValue([makeBluetoothConfig()]);

    const { getByRole } = render(<DevicesView />);
    await waitFor(() =>
      expect(getByRole("button", { name: "Connect" })).toBeTruthy(),
    );
  });

  it("hides provisioning prompt for a configured connected device", async () => {
    listBluetoothDevices.mockResolvedValue([makeDevice({ connected: true })]);
    listConfigs.mockResolvedValue([makeBluetoothConfig()]);

    const { getByRole, queryByRole, queryByText } = render(<DevicesView />);
    await waitFor(() =>
      expect(getByRole("button", { name: "Disconnect" })).toBeTruthy(),
    );

    expect(queryByRole("button", { name: "Retry provisioning" })).toBeNull();
    expect(queryByText(/no managed playback output is visible/)).toBeNull();
  });

  it("shows Disconnect for a connected device", async () => {
    listBluetoothDevices.mockResolvedValue([makeDevice({ connected: true })]);

    const { getByRole, queryByRole } = render(<DevicesView />);
    await waitFor(() =>
      expect(getByRole("button", { name: "Disconnect" })).toBeTruthy(),
    );

    expect(queryByRole("button", { name: "Connect" })).toBeNull();
    fireEvent.click(getByRole("button", { name: "Disconnect" }));
    await waitFor(() =>
      expect(disconnect).toHaveBeenCalledWith("AA:BB:CC:DD:EE:FF"),
    );
  });

  it("keeps Bluetooth removal behind advanced confirmation", async () => {
    listBluetoothDevices.mockResolvedValue([makeDevice({ connected: true })]);

    const { getByRole, getByText, queryByRole } = render(<DevicesView />);
    await waitFor(() =>
      expect(getByRole("button", { name: "Disconnect" })).toBeTruthy(),
    );
    expect(queryByRole("button", { name: "Remove output" })).toBeNull();
    fireEvent.click(getByText("More"));
    fireEvent.click(
      await waitFor(() => getByRole("button", { name: "Remove output" })),
    );
    expect(getByRole("dialog")).toBeTruthy();
    fireEvent.click(getByRole("button", { name: "Cancel" }));
    expect(queryByRole("dialog")).toBeNull();
  });
  it("keeps system outputs out of the playback controls", async () => {
    listDevices.mockResolvedValue([
      makeOutput(),
      {
        ...makeOutput({ id: 9, name: "visualizer" }),
        role: "system",
        selectable: false,
      } as OutputDevice,
    ]);

    const { getByRole, queryByRole } = render(<DevicesView />);
    await waitFor(() =>
      expect(
        getByRole("button", { name: "Enable DSP for USB DAC" }),
      ).toBeTruthy(),
    );
    expect(
      queryByRole("button", { name: "Enable DSP for visualizer" }),
    ).toBeNull();
  });

  it("shows Pair for an unpaired Bluetooth candidate", async () => {
    listBluetoothDevices.mockResolvedValue([makeDevice({ paired: false })]);

    const { getByRole } = render(<DevicesView />);
    const pairButton = await waitFor(() =>
      getByRole("button", { name: "Pair" }),
    );
    fireEvent.click(pairButton);
    await waitFor(() => expect(pair).toHaveBeenCalledWith("AA:BB:CC:DD:EE:FF"));
  });

  it("reports Bluetooth 503 as unavailable and retries the terminal state", async () => {
    listBluetoothDevices
      .mockRejectedValueOnce(new ApiError(503, "BlueZ is unavailable"))
      .mockResolvedValueOnce([]);

    const { getByRole } = render(<DevicesView />);
    const retry = await waitFor(() =>
      getByRole("button", { name: "Retry Bluetooth" }),
    );
    fireEvent.click(retry);
    await waitFor(() => expect(listBluetoothDevices).toHaveBeenCalledTimes(2));
  });
});
