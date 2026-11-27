import { invoke } from "@tauri-apps/api/core";

export type Language = "system" | "en" | "es" | "fr" | "pt" | "zh" | "hi" | "ar" | "bn";
export type Mode = "dark" | "light" | "system";
export type SchemeSource = "wallpaper" | "accent" | "custom";
export const VARIANTS = [
  "tonalSpot", "vibrant", "expressive", "neutral", "monochrome",
  "fidelity", "content", "rainbow", "fruitSalad",
] as const;
export type VariantId = typeof VARIANTS[number];

export interface Entry { id: string; enabled: boolean }
export interface Scale { scale: number }

/** Mirrors `norbanner_config::Config` (`shell.json`). */
export interface Config {
  version: number;
  appearance: {
    rounding: Scale;
    spacing: Scale;
    padding: Scale;
    font: { family: { sans: string; mono: string; material: string }; size: Scale };
    animScale: number;
    transparency: { enabled: boolean; base: number; layers: number };
  };
  theme: { source: SchemeSource; sourceColor: string; mode: Mode; variant: string };
  general: {
    language: Language;
    replaceTaskbar: boolean;
    reserveSpace: boolean;
    checkUpdates: boolean;
    autoUpdate: boolean;
  };
  border: { thickness: number; rounding: number; smoothing: number };
  bar: {
    persistent: boolean;
    /** Image file for the logo; empty uses Norbanner's. */
    logo: string;
    showOnHover: boolean;
    dragThreshold: number;
    entries: Entry[];
    statusIcons: Entry[];
    workspaces: { shown: number; activeIndicator: boolean; showWindows: boolean; maxWindowIcons: number };
    clock: { showIcon: boolean; showDate: boolean; showSeconds: boolean };
    scrollActions: { workspaces: boolean; volume: boolean; brightness: boolean };
    popouts: { activeWindow: boolean; tray: boolean; statusIcons: boolean };
  };
  dashboard: { enabled: boolean; showOnHover: boolean; dragThreshold: number; resourceUpdateInterval: number };
  launcher: {
    enabled: boolean; showOnHover: boolean; dragThreshold: number; maxShown: number;
    actionPrefix: string; hotkey: string; enableCalculator: boolean;
  };
  osd: { enabled: boolean; hideDelay: number; enableBrightness: boolean; enableMicrophone: boolean };
  session: { enabled: boolean; dragThreshold: number; hotkey: string };
  services: {
    useTwelveHourClock: boolean; maxVolume: number; audioIncrement: number;
    brightnessIncrement: number; weatherLocation: string; useFahrenheit: boolean;
  };
  paths: { wallpaperDir: string };
}

/** A nested partial: only what changes is sent; objects merge on save. */
export type Patch<T> = { [K in keyof T]?: T[K] extends unknown[] ? T[K] : T[K] extends object ? Patch<T[K]> : T[K] };

/** Applies `patch` to `config` the way `norbanner_config::patch` merges it. */
export function merge<T>(config: T, patch: Patch<T>): T {
  const out: Record<string, unknown> = { ...(config as Record<string, unknown>) };
  for (const [key, value] of Object.entries(patch as Record<string, unknown>)) {
    const current = out[key];
    out[key] = value !== null && typeof value === "object" && !Array.isArray(value)
      && current !== null && typeof current === "object" && !Array.isArray(current)
      ? merge(current, value as Patch<typeof current>)
      : value;
  }
  return out as T;
}

export type Palette = Record<string, string>;

export const loadConfig = () => invoke<Config>("get_config");
export const defaultConfig = () => invoke<Config>("default_config");
export const saveConfig = (patch: Patch<Config>) => invoke<Config>("set_config", { patch });
export const schemePreview = (theme: Config["theme"]) => invoke<Palette>("scheme_preview", { theme });
/** Windows' open dialog for an image; the path, or null when cancelled. */
export const pickLogo = (filter: string) => invoke<string | null>("pick_logo", { filter });
/** The same dialog, for a wallpaper. */
export const pickImage = pickLogo;
/** Sets an image file from the computer as the wallpaper. */
export const applyWallpaperFile = async (path: string) => {
  await invoke<void>("apply_wallpaper_file", { path });
  wallpaperRequest = undefined;
  wallpaperSelectionRequest = undefined;
};
/** A data URI preview of the image at `path`, or null if it cannot be shown. */
export const logoPreview = (path: string) => invoke<string | null>("logo_preview", { path });
export const getAutostart = () => invoke<boolean>("get_autostart");
export const setAutostart = (enabled: boolean) => invoke<void>("set_autostart", { enabled });

export interface UpdateInfo {
  version: string;
  /** Release page with the notes, if the manifest has one. */
  notes: string | null;
}

/** Resolves to the newer version available, or null when up to date. */
export const checkUpdate = () => invoke<UpdateInfo | null>("check_update");
export const installUpdate = () => invoke<void>("install_update");
export const openReleaseNotes = (url: string) => invoke<void>("open_release_notes", { url });
/** Fixed project URL, opened only when the user asks. */
export const openRepository = () => invoke<void>("open_repository");

let wallpaperRequest: Promise<string | null> | undefined;
/** One bounded native read per Settings process, including React StrictMode. */
export const loadWallpaperPreview = () =>
  wallpaperRequest ??= invoke<string | null>("wallpaper_preview").catch(() => null);
/** Reads the wallpaper again next time (it may have changed outside Settings). */
export const forgetWallpaperPreview = () => { wallpaperRequest = undefined; };
let wallpaperSelectionRequest: Promise<number | null> | undefined;
/** Share overlapping reads from StrictMode or quick section navigation. */
export const loadWallpaperSelection = () => {
  if (wallpaperSelectionRequest) return wallpaperSelectionRequest;
  const request = invoke<number | null>("wallpaper_selection").finally(() => {
    if (wallpaperSelectionRequest === request) wallpaperSelectionRequest = undefined;
  });
  wallpaperSelectionRequest = request;
  return request;
};
export const applyWallpaper = async (id: number, preview: string) => {
  await invoke<void>("apply_wallpaper", { id });
  // Original images stay in native storage; Settings uses only the small preview.
  wallpaperRequest = Promise.resolve(preview);
  wallpaperSelectionRequest = undefined;
};
