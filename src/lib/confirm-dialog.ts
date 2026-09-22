import { isTauri } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";

type ConfirmDialogOptions = {
  title?: string;
  okLabel?: string;
  cancelLabel?: string;
};

/** Native confirm in Tauri; falls back to window.confirm in the browser. */
export async function confirmDialog(
  message: string,
  options: ConfirmDialogOptions = {},
): Promise<boolean> {
  if (!isTauri()) {
    return window.confirm(message);
  }
  return ask(message, {
    title: options.title ?? "Sprite Studio",
    kind: "warning",
    okLabel: options.okLabel ?? "OK",
    cancelLabel: options.cancelLabel ?? "Cancel",
  });
}
