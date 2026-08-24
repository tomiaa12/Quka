import { LogicalSize } from "@tauri-apps/api/dpi";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function hideSearchWindow(): Promise<void> {
  if (!isTauri()) return;
  await getCurrentWindow().hide();
}

export async function showSearchWindow(): Promise<void> {
  if (!isTauri()) return;
  const current = getCurrentWindow();
  await current.show();
  await current.setFocus();
}

export async function resizeSearchWindow(width: number, height: number): Promise<void> {
  if (!isTauri()) return;
  await getCurrentWindow().setSize(new LogicalSize(width, height));
}

export async function onWindowFocusChange(
  handler: (focused: boolean) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) return () => undefined;
  return getCurrentWindow().onFocusChanged(({ payload }) => handler(payload));
}

export async function isSearchWindowFocused(): Promise<boolean> {
  if (!isTauri()) return document.hasFocus();
  try {
    return await getCurrentWindow().isFocused();
  } catch {
    return document.hasFocus();
  }
}
