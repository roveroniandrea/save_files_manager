// Typed wrappers over Tauri commands (src-tauri/src/commands.rs).
import { invoke } from "@tauri-apps/api/core";
import type { AppError, Config, Settings } from "./types";

export const getConfig = () => invoke<Config>("get_config");

export const updateSettings = (settings: Settings) =>
  invoke<Config>("update_settings", { settings });

/** Extracts a readable message from a rejected command. */
export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) return (e as AppError).message;
  return String(e);
}
