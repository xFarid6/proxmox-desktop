import { describe, expect, it } from "vitest";
import { formatBytes, formatUptime, percent } from "./format";

describe("formatBytes", () => {
  it("returns em dash for missing input", () => {
    expect(formatBytes(undefined)).toBe("—");
  });

  it("stays in bytes below 1024", () => {
    expect(formatBytes(512)).toBe("512 B");
  });

  it("rounds sub-100 values to one decimal", () => {
    expect(formatBytes(1536)).toBe("1.5 KiB");
  });

  it("rounds values >= 100 to a whole number", () => {
    expect(formatBytes(1024 * 150)).toBe("150 KiB");
  });

  it("climbs units up to TiB", () => {
    expect(formatBytes(1024 ** 4 * 2)).toBe("2.0 TiB");
  });

  it("stops climbing at the largest unit instead of overflowing it", () => {
    expect(formatBytes(1024 ** 5)).toBe("1024 TiB");
  });
});

describe("formatUptime", () => {
  it("returns em dash for zero or missing", () => {
    expect(formatUptime(undefined)).toBe("—");
    expect(formatUptime(0)).toBe("—");
  });

  it("shows minutes under an hour", () => {
    expect(formatUptime(150)).toBe("2m");
  });

  it("shows hours and minutes under a day", () => {
    expect(formatUptime(3 * 3600 + 61)).toBe("3h 1m");
  });

  it("shows days and hours at a day or more", () => {
    expect(formatUptime(2 * 86400 + 3600)).toBe("2d 1h");
  });
});

describe("percent", () => {
  it("is zero when used or max is missing", () => {
    expect(percent(undefined, 100)).toBe(0);
    expect(percent(50, undefined)).toBe(0);
    expect(percent(50, 0)).toBe(0);
  });

  it("rounds to the nearest whole percent", () => {
    expect(percent(1, 3)).toBe(33);
    expect(percent(2, 3)).toBe(67);
  });

  it("handles exact and over-100 ratios", () => {
    expect(percent(50, 50)).toBe(100);
    expect(percent(150, 100)).toBe(150);
  });
});
