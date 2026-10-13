import { invoke } from "@tauri-apps/api/core";
export const isDesktop =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
export async function native<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isDesktop)
    throw new Error(
      "Open PulseDeck in the desktop app to use the Rust audio engine.",
    );
  return invoke<T>(command, args);
}
