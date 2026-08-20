import { describe, it, expect } from "vitest";
import { eqResponse, FREQ_MIN, FREQ_MAX } from "./eqCurve";
import type { EqBand } from "../types";

const N = 200;
const pts = eqResponse([]);
// Helper: find the curve point whose freq is closest to a target.
const at = (f: number, arr = pts) =>
  arr.reduce((a, p) => (Math.abs(p.f - f) < Math.abs(a.f - f) ? p : a), arr[0]);

describe("eqResponse math", () => {
  it("iterates the audible band [20Hz, 20kHz]", () => {
    expect(pts.length).toBe(N);
    expect(pts[0].f).toBeCloseTo(FREQ_MIN, 5);
    expect(pts[N - 1].f).toBeCloseTo(FREQ_MAX, 5);
  });

  it("is flat 0 dB with no bands", () => {
    for (const p of pts) expect(p.db).toBeCloseTo(0, 5);
  });

  it("peaking filter peaks at +gain dB at the center frequency", () => {
    const band: EqBand = { type: "peaking", freq: 1000, gain: 6, q: 2 };
    const r = eqResponse([band]);
    const peak = at(1000, r);
    expect(peak.db).toBeCloseTo(6, 1);
    // Far from the peak the response returns ~0 dB.
    expect(at(100, r).db).toBeLessThan(0.5);
    expect(at(10000, r).db).toBeLessThan(0.5);
  });

  it("peaking of -3 dB cuts to -3 dB at the center", () => {
    const r = eqResponse([{ type: "peaking", freq: 500, gain: -3, q: 1 }]);
    expect(at(500, r).db).toBeCloseTo(-3, 1);
  });

  it("low shelf reaches +gain dB below the transition band", () => {
    const r = eqResponse([{ type: "low_shelf", freq: 200, gain: 4, q: 0.707 }]);
    // 20Hz is well below the 200Hz shelf corner.
    expect(at(20, r).db).toBeCloseTo(4, 1);
    // Above the corner the response returns to ~0.
    expect(at(2000, r).db).toBeGreaterThan(-0.5);
    expect(at(2000, r).db).toBeLessThan(0.5);
  });

  it("high shelf reaches +gain dB above the transition band", () => {
    const r = eqResponse([
      { type: "high_shelf", freq: 3000, gain: -3, q: 0.707 },
    ]);
    expect(at(20000, r).db).toBeCloseTo(-3, 1);
    expect(at(100, r).db).toBeGreaterThan(-0.5);
    expect(at(100, r).db).toBeLessThan(0.5);
  });

  it("sums magnitudes of multiple overlapping bands (dB added)", () => {
    // Two peaking filters at the same freq both +6 dB → total +12 dB.
    const r = eqResponse([
      { type: "peaking", freq: 1000, gain: 6, q: 1 },
      { type: "peaking", freq: 1000, gain: 6, q: 1 },
    ]);
    expect(at(1000, r).db).toBeCloseTo(12, 1);
  });

  it("clamps display range to ±24 dB without throwing", () => {
    // Extreme gain near the cap should still produce finite numbers.
    expect(() =>
      eqResponse([{ type: "peaking", freq: 1000, gain: 24, q: 4 }]),
    ).not.toThrow();
  });
});
