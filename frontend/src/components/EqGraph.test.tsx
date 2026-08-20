import { render } from "@testing-library/react";
import { EqGraph } from "./EqGraph";

describe("EqGraph", () => {
  it("renders an svg with the response curve and axes when bands present", () => {
    const bands = [
      { type: "low_shelf" as const, freq: 200, gain: 4, q: 0.707 },
      { type: "peaking" as const, freq: 1000, gain: -3, q: 1 },
      { type: "high_shelf" as const, freq: 5000, gain: -2, q: 0.707 },
    ];
    const { container } = render(<EqGraph bands={bands} />);
    const svg = container.querySelector("svg");
    expect(svg).not.toBeNull();
    // Response curve path
    const curve = container.querySelector(
      'path[class*="curve"]',
    ) as SVGPathElement | null;
    expect(curve).not.toBeNull();
    expect(curve!.getAttribute("d")).toMatch(/^M/);
    expect((curve!.getAttribute("d") || "").split(" ").length).toBeGreaterThan(
      10,
    );
    // Band guide lines (one per band)
    const guides = container.querySelectorAll('line[class*="bandGuide"]');
    expect(guides.length).toBe(bands.length);
    // Frequency + dB tick labels exist
    expect(
      container.querySelectorAll('text[class*="tickLbl"]').length,
    ).toBeGreaterThan(8);
  });

  it("renders a flat curve when there are no bands", () => {
    const { container } = render(<EqGraph bands={[]} />);
    const curve = container.querySelector(
      'path[class*="curve"]',
    ) as SVGPathElement | null;
    expect(curve).not.toBeNull();
    // Flat 0 dB line: every Y should match the baseline within epsilon.
    const d = curve!.getAttribute("d") || "";
    const ys = (d.match(/,(\d+\.\d+)/g) || []).map((s) =>
      parseFloat(s.slice(1)),
    );
    const max = Math.max(...ys);
    const min = Math.min(...ys);
    expect(max - min).toBeLessThan(0.5);
  });
});
