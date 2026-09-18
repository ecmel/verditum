import { vi } from "vitest";

// Browser tests exercise the real UI without a native Tauri process.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async () => {}),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

vi.mock("@tauri-apps/plugin-log", () => ({
  info: vi.fn(async () => {}),
  warn: vi.fn(async () => {}),
  error: vi.fn(async () => {}),
}));
