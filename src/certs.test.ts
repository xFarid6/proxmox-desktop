import { describe, expect, it } from "vitest";
import { EXPIRY_WARN_DAYS, expiryLabel, expiryState } from "./certs";

// A fixed "now" so the thresholds are exact rather than clock-dependent.
const NOW = Date.UTC(2026, 6, 30, 12, 0, 0);
const at = (days: number) => (NOW + days * 86_400_000) / 1000;

describe("expiryState", () => {
  it("flags a certificate whose notafter has passed", () => {
    expect(expiryState(at(-1), NOW)).toBe("expired");
  });

  it("treats the exact expiry instant as expired, not as expiring", () => {
    expect(expiryState(NOW / 1000, NOW)).toBe("expired");
  });

  it("warns inside the window and stays quiet outside it", () => {
    expect(expiryState(at(1), NOW)).toBe("expiring");
    expect(expiryState(at(EXPIRY_WARN_DAYS), NOW)).toBe("expiring");
    expect(expiryState(at(EXPIRY_WARN_DAYS + 1), NOW)).toBe("ok");
    expect(expiryState(at(400), NOW)).toBe("ok");
  });

  it("does not read a missing notafter as healthy", () => {
    expect(expiryState(undefined, NOW)).toBe("unknown");
  });
});

describe("expiryLabel", () => {
  it("counts days left, singular at one", () => {
    expect(expiryLabel(at(45), NOW)).toBe("45 days left");
    expect(expiryLabel(at(1.5), NOW)).toBe("1 day left");
  });

  it("collapses sub-day windows to today, either side of expiry", () => {
    expect(expiryLabel(at(0.25), NOW)).toBe("expires today");
    expect(expiryLabel(at(-0.25), NOW)).toBe("expired today");
  });

  it("counts days since expiry", () => {
    expect(expiryLabel(at(-3), NOW)).toBe("expired 3 days ago");
    expect(expiryLabel(at(-1.2), NOW)).toBe("expired 1 day ago");
  });

  it("says so when the API reported no expiry", () => {
    expect(expiryLabel(undefined, NOW)).toBe("no expiry reported");
  });
});
