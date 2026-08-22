import { describe, it, expect } from "vitest";
import { api } from "../api";

describe("api.coverForDir", () => {
  it("encodes the absolute folder path for the wildcard route", () => {
    expect(api.coverForDir("/mnt/music1/Pop/Jamiroquai")).toBe(
      "/api/library/cover/%2Fmnt%2Fmusic1%2FPop%2FJamiroquai",
    );
  });
  it("encodes spaces, percent signs and hashes in folder names", () => {
    const dir = "/mnt/music1/Pop/The Return Of The Space Cowboy (CD2)";
    const url = api.coverForDir(dir);
    expect(url).toContain("%20");
    expect(decodeURIComponent(url)).toBe(`/api/library/cover/${dir}`);
  });
});
