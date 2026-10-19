import { invoke } from "@tauri-apps/api/core";
export const isDesktop =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
export async function native<T>(
  command: "control_request" | "ui_publish" | "ui_reply",
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isDesktop)
    throw new Error(
      "Open PulseDeck in the desktop app to use the Rust audio engine.",
    );
  return invoke<T>(command, args);
}

export async function control<T>(
  command: string,
  args: Record<string, unknown> = {},
  guards: { expected_load_revision?: number } = {},
): Promise<T> {
  const response = await native<{
    ok: boolean;
    result?: T;
    error?: { message: string };
  }>("control_request", { request: { command, args, ...guards } });
  if (!response.ok)
    throw new Error(response.error?.message || "Command failed");
  return response.result as T;
}
