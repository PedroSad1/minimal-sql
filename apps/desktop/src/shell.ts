import { open, save } from "@tauri-apps/plugin-dialog";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";

export async function pickSqliteFile() {
  return open({
    multiple: false,
    directory: false,
    filters: [{ name: "SQLite", extensions: ["db", "sqlite", "sqlite3"] }],
  });
}

export async function pickSavePath(defaultName: string) {
  return save({ defaultPath: defaultName });
}

export async function pickOpenFile() {
  return open({
    multiple: false,
    directory: false,
    filters: [{ name: "Data", extensions: ["csv", "json", "xlsx"] }],
  });
}

export async function copyText(text: string) {
  await writeText(text);
}

export async function pasteText() {
  return readText();
}

export async function maximizeWindow() {
  await getCurrentWindow().maximize();
}

export function onOpenSqlite(handler: () => void) {
  return listen("menu://open-sqlite", handler);
}
