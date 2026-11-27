import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  checkUpdate, defaultConfig, forgetWallpaperPreview, getAutostart, installUpdate, loadConfig, loadWallpaperPreview, logoPreview, merge,
  openReleaseNotes, openRepository, pickLogo, saveConfig, schemePreview, setAutostart,
  VARIANTS, type Config, type Palette, type Patch, type UpdateInfo,
} from "./config";
import { Card, ColourInput, ListEditor, Row, Segmented, Select, Slider, Stepper, SubRow, Toggle } from "./controls";
import { LANGUAGE_OPTIONS, resolveLanguage, textsFor, type Texts } from "./i18n";
import { UpdateBanner } from "./UpdateBanner";
import { WallpaperGallery } from "./WallpaperGallery";
import norbannerLogo from "./assets/norbanner-logo.png";

export type Section = "style" | "bar" | "panels" | "look" | "general" | "about";
const SECTIONS: readonly Section[] = ["style", "bar", "panels", "look", "general", "about"];

/** Line icons for the navigation pane, drawn on a 24-unit grid. */
function NavIcon({ section }: { section: Section }) {
  const paths: Record<Section, ReactNode> = {
    style: <><circle cx="12" cy="12" r="8.5" /><circle cx="8.5" cy="10" r="1.3" /><circle cx="12" cy="7.5" r="1.3" /><circle cx="15.5" cy="10" r="1.3" /><path d="M12 20.5c-1.5 0-2-1.2-1.4-2.3.7-1.3-.1-2.7-1.6-2.7H7" /></>,
    bar: <><rect x="3.5" y="3.5" width="17" height="17" rx="4" /><path d="M8.5 3.5v17" /><circle cx="6" cy="7.5" r=".6" /><circle cx="6" cy="10.5" r=".6" /></>,
    panels: <><rect x="3.5" y="3.5" width="17" height="17" rx="4" /><path d="M8 3.5v4.5a2 2 0 0 0 2 2h4a2 2 0 0 0 2-2V3.5M9 20.5V17a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v3.5" /></>,
    look: <><path d="M4 7h10M18 7h2M4 17h4M12 17h8" /><circle cx="16" cy="7" r="2" /><circle cx="10" cy="17" r="2" /></>,
    general: <><circle cx="12" cy="12" r="3" /><path d="M12 3.5v2.5M12 18v2.5M3.5 12H6M18 12h2.5M6 6l1.8 1.8M16.2 16.2 18 18M6 18l1.8-1.8M16.2 7.8 18 6" /></>,
    about: <><circle cx="12" cy="12" r="8.5" /><path d="M12 11v5.5" /><circle cx="12" cy="8" r=".8" /></>,
  };
  return <svg className="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[section]}</svg>;
}

function sectionLabel(t: Texts, section: Section): string {
  return {
    style: t.navStyle, bar: t.navBar, panels: t.navPanels, look: t.navLook, general: t.navGeneral, about: t.navAbout,
  }[section];
}

/** Maps the shell's Material roles onto the window's CSS variables. */
function paletteStyle(palette: Palette | null): CSSProperties {
  if (!palette) return {};
  const vars: Record<string, string> = {};
  for (const [role, colour] of Object.entries(palette)) {
    vars[`--m3-${role.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`] = colour;
  }
  return vars as CSSProperties;
}

const HOTKEYS = ["super", "alt+space", "ctrl+space", "none"] as const;

export function App() {
  const [config, setConfigState] = useState<Config | null>(null);
  const [palette, setPalette] = useState<Palette | null>(null);
  const [autostart, setAutostartState] = useState<boolean | undefined>(undefined);
  const [section, setSection] = useState<Section>("style");
  const [error, setError] = useState<string | null>(null);
  const [wallpaper, setWallpaper] = useState<string | null>(null);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [updateHidden, setUpdateHidden] = useState(false);
  const [checking, setChecking] = useState<"idle" | "checking" | "current" | "failed">("idle");
  const [installing, setInstalling] = useState(false);
  const [installFailed, setInstallFailed] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);
  const [version, setVersion] = useState("");
  const [customLogo, setCustomLogo] = useState<string | null>(null);
  const pending = useRef<Patch<Config>>({});
  const timer = useRef<number | undefined>(undefined);

  const t = useMemo(
    () => textsFor(resolveLanguage(config?.general.language ?? "system", navigator.language)),
    [config?.general.language],
  );

  useEffect(() => {
    loadConfig().then(setConfigState).catch((e) => setError(String(e)));
    getAutostart().then(setAutostartState).catch(() => setAutostartState(false));
    loadWallpaperPreview().then(setWallpaper);
    getVersion().then(setVersion).catch(() => {});
  }, []);

  // Back in Settings after changing the wallpaper elsewhere: show the new one
  // and the colours made from it.
  useEffect(() => {
    let stop: (() => void) | undefined;
    getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (!focused) return;
      forgetWallpaperPreview();
      loadWallpaperPreview().then(setWallpaper);
    }).then((unlisten) => { stop = unlisten; }).catch(() => {});
    return () => stop?.();
  }, []);

  // A preview of the logo the bar shows, when it is the user's own image.
  const logoPath = config?.bar.logo ?? "";
  useEffect(() => {
    let alive = true;
    if (!logoPath) { setCustomLogo(null); return; }
    logoPreview(logoPath).then((uri) => { if (alive) setCustomLogo(uri); }).catch(() => { if (alive) setCustomLogo(null); });
    return () => { alive = false; };
  }, [logoPath]);

  // The window wears the colours the shell will use.
  const theme = config?.theme;
  useEffect(() => {
    if (!theme) return;
    let alive = true;
    schemePreview(theme).then((p) => { if (alive) setPalette(p); }).catch(() => {});
    return () => { alive = false; };
  }, [theme?.source, theme?.sourceColor, theme?.mode, theme?.variant, wallpaper]);

  const checkForUpdate = useCallback(() => {
    setChecking("checking");
    checkUpdate()
      .then((info) => { setUpdate(info); setChecking(info ? "idle" : "current"); })
      .catch(() => setChecking("failed"));
  }, []);

  useEffect(() => {
    if (config?.general.checkUpdates) checkForUpdate();
    // Only once, when the configuration first arrives.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config === null]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<{ downloaded: number; total: number | null }>("update-progress", (event) => {
      const { downloaded, total } = event.payload;
      setProgress(total ? downloaded / total : null);
    }).then((stop) => { unlisten = stop; }).catch(() => {});
    return () => unlisten?.();
  }, []);

  /** Applies a change at once and saves it shortly after (sliders send bursts). */
  const change = useCallback((patch: Patch<Config>) => {
    setConfigState((current) => current && merge(current, patch));
    pending.current = merge(pending.current as Config, patch) as Patch<Config>;
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      const send = pending.current;
      pending.current = {};
      saveConfig(send).then((saved) => { setConfigState(saved); setError(null); })
        .catch((e) => setError(t.saveError.replace("{error}", String(e))));
    }, 140);
  }, [t]);

  const reset = useCallback(() => {
    defaultConfig().then((defaults) => {
      saveConfig(defaults as Patch<Config>).then(setConfigState).catch((e) => setError(String(e)));
    });
  }, []);

  if (!config) {
    return <div className="app loading">{error ?? "…"}</div>;
  }

  const entryNames = t.entryNames as Record<string, string>;
  const statusNames = t.statusNames as Record<string, string>;
  const hotkeyLabel = (value: string) =>
    value === "super" ? t.hotkeySuper : value === "none" ? t.hotkeyNone : value.replace(/\b\w/g, (c) => c.toUpperCase());

  const pages: Record<Section, ReactNode> = {
    style: <>
      <div className="hero">
        <div className="hero-wallpaper" style={wallpaper ? { backgroundImage: `url(${wallpaper})` } : undefined}>
          <div className="hero-frame" />
        </div>
        {palette ? <div className="swatches" aria-label={t.palette}>
          {["primary", "secondary", "tertiary", "primaryContainer", "secondaryContainer", "tertiaryContainer", "surfaceContainerHigh", "error"]
            .map((role) => <span key={role} className="swatch" title={role} style={{ background: palette[role] }} />)}
        </div> : null}
      </div>
      <Card title={t.colourSource}>
        <Row label={t.colourSource} hint={t.colourSourceHint} stacked>
          <Segmented label={t.colourSource} value={config.theme.source}
            options={[
              { value: "wallpaper", label: t.sourceWallpaper },
              { value: "accent", label: t.sourceAccent },
              { value: "custom", label: t.sourceCustom },
            ]}
            onChange={(source) => change({ theme: { source } })} />
        </Row>
        <SubRow open={config.theme.source === "custom"} label={t.customColour}>
          <ColourInput label={t.customColour} value={config.theme.sourceColor}
            onChange={(sourceColor) => change({ theme: { sourceColor } })} />
        </SubRow>
        <Row label={t.mode}>
          <Segmented label={t.mode} value={config.theme.mode}
            options={[
              { value: "dark", label: t.modeDark },
              { value: "light", label: t.modeLight },
              { value: "system", label: t.modeSystem },
            ]}
            onChange={(mode) => change({ theme: { mode } })} />
        </Row>
        <Row label={t.variant} stacked>
          <div className="chips" role="radiogroup" aria-label={t.variant}>
            {VARIANTS.map((variant, i) => (
              <button key={variant} type="button" role="radio" aria-checked={config.theme.variant === variant}
                className="chip" onClick={() => change({ theme: { variant } })}>
                {(t.variantNames as string[])[i] ?? variant}
              </button>
            ))}
          </div>
        </Row>
      </Card>
      <WallpaperGallery texts={t} onApplied={setWallpaper} />
      <Card title={t.transparency}>
        <Row label={t.transparency} hint={t.transparencyHint}>
          <Toggle label={t.transparency} checked={config.appearance.transparency.enabled}
            onChange={(enabled) => change({ appearance: { transparency: { enabled } } })} />
        </Row>
        <SubRow open={config.appearance.transparency.enabled} label={t.transparencyBase}>
          <Slider label={t.transparencyBase} unit="%" min={20} max={100}
            value={Math.round(config.appearance.transparency.base * 100)}
            onChange={(v) => change({ appearance: { transparency: { base: v / 100 } } })} />
        </SubRow>
        <SubRow open={config.appearance.transparency.enabled} label={t.transparencyLayers}>
          <Slider label={t.transparencyLayers} unit="%" min={10} max={100}
            value={Math.round(config.appearance.transparency.layers * 100)}
            onChange={(v) => change({ appearance: { transparency: { layers: v / 100 } } })} />
        </SubRow>
      </Card>
    </>,
    bar: <>
      <Card>
        <Row label={t.barPersistent} hint={t.barPersistentHint}>
          <Toggle label={t.barPersistent} checked={config.bar.persistent}
            onChange={(persistent) => change({ bar: { persistent } })} />
        </Row>
        <SubRow open={!config.bar.persistent} label={t.showOnHover}>
          <Toggle label={t.showOnHover} checked={config.bar.showOnHover}
            onChange={(showOnHover) => change({ bar: { showOnHover } })} />
        </SubRow>
      </Card>
      <Card title={t.barLogo}>
        <div className="logo-picker">
          <img className="logo-preview" src={config.bar.logo ? customLogo ?? norbannerLogo : norbannerLogo} alt="" />
          <div className="logo-text">
            <span className="row-label">{config.bar.logo ? t.logoCustom : t.logoDefault}</span>
            <span className={config.bar.logo ? "row-hint logo-path" : "row-hint"} title={config.bar.logo || undefined}>{config.bar.logo || t.barLogoHint}</span>
          </div>
          <div className="logo-actions">
            {config.bar.logo ? (
              <button type="button" className="text-link" onClick={() => change({ bar: { logo: "" } })}>{t.logoReset}</button>
            ) : null}
            <button type="button" className="tonal-button"
              onClick={() => { pickLogo(t.logoImages).then((path) => { if (path) change({ bar: { logo: path } }); }).catch(() => {}); }}>
              {t.logoChoose}
            </button>
          </div>
        </div>
      </Card>
      <Card title={t.barEntries}>
        <ListEditor items={config.bar.entries} names={entryNames} upLabel={t.moveUp} downLabel={t.moveDown}
          onChange={(entries) => change({ bar: { entries } })} />
      </Card>
      <Card title={t.workspaces}>
        <Row label={t.workspacesShowWindows}>
          <Toggle label={t.workspacesShowWindows} checked={config.bar.workspaces.showWindows}
            onChange={(showWindows) => change({ bar: { workspaces: { showWindows } } })} />
        </Row>
        <SubRow open={config.bar.workspaces.showWindows} label={t.workspacesApps}>
          <Stepper label={t.workspacesApps} min={1} max={20} value={config.bar.workspaces.maxWindowIcons}
            onChange={(maxWindowIcons) => change({ bar: { workspaces: { maxWindowIcons } } })} />
        </SubRow>
      </Card>
      <Card title={t.clock}>
        <Row label={t.clockIcon}>
          <Toggle label={t.clockIcon} checked={config.bar.clock.showIcon}
            onChange={(showIcon) => change({ bar: { clock: { showIcon } } })} />
        </Row>
        <Row label={t.clockDate}>
          <Toggle label={t.clockDate} checked={config.bar.clock.showDate}
            onChange={(showDate) => change({ bar: { clock: { showDate } } })} />
        </Row>
        <Row label={t.clockSeconds}>
          <Toggle label={t.clockSeconds} checked={config.bar.clock.showSeconds}
            onChange={(showSeconds) => change({ bar: { clock: { showSeconds } } })} />
        </Row>
        <Row label={t.clock12}>
          <Toggle label={t.clock12} checked={config.services.useTwelveHourClock}
            onChange={(useTwelveHourClock) => change({ services: { useTwelveHourClock } })} />
        </Row>
      </Card>
      <Card title={t.statusIcons}>
        <ListEditor items={config.bar.statusIcons} names={statusNames} upLabel={t.moveUp} downLabel={t.moveDown}
          onChange={(statusIcons) => change({ bar: { statusIcons } })} />
      </Card>
      <Card title={t.scroll}>
        <Row label={t.scrollWorkspaces}>
          <Toggle label={t.scrollWorkspaces} checked={config.bar.scrollActions.workspaces}
            onChange={(workspaces) => change({ bar: { scrollActions: { workspaces } } })} />
        </Row>
        <Row label={t.scrollVolume}>
          <Toggle label={t.scrollVolume} checked={config.bar.scrollActions.volume}
            onChange={(volume) => change({ bar: { scrollActions: { volume } } })} />
        </Row>
        <Row label={t.scrollBrightness}>
          <Toggle label={t.scrollBrightness} checked={config.bar.scrollActions.brightness}
            onChange={(brightness) => change({ bar: { scrollActions: { brightness } } })} />
        </Row>
      </Card>
    </>,
    panels: <>
      <Card title={t.dashboard}>
        <Row label={t.enabled} hint={t.dashboardHint}>
          <Toggle label={t.dashboard} checked={config.dashboard.enabled}
            onChange={(enabled) => change({ dashboard: { enabled } })} />
        </Row>
        <SubRow open={config.dashboard.enabled} label={t.showOnHover}>
          <Toggle label={t.showOnHover} checked={config.dashboard.showOnHover}
            onChange={(showOnHover) => change({ dashboard: { showOnHover } })} />
        </SubRow>
      </Card>
      <Card title={t.launcher}>
        <Row label={t.enabled} hint={t.launcherHint}>
          <Toggle label={t.launcher} checked={config.launcher.enabled}
            onChange={(enabled) => change({ launcher: { enabled } })} />
        </Row>
        <Row label={t.hotkey}>
          <Select label={t.hotkey} value={config.launcher.hotkey}
            options={HOTKEYS.map((value) => ({ value, label: hotkeyLabel(value) }))}
            onChange={(hotkey) => change({ launcher: { hotkey } })} />
        </Row>
        <Row label={t.maxShown}>
          <Stepper label={t.maxShown} min={3} max={12} value={config.launcher.maxShown}
            onChange={(maxShown) => change({ launcher: { maxShown } })} />
        </Row>
        <Row label={t.calculator}>
          <Toggle label={t.calculator} checked={config.launcher.enableCalculator}
            onChange={(enableCalculator) => change({ launcher: { enableCalculator } })} />
        </Row>
      </Card>
      <Card title={t.osd}>
        <Row label={t.enabled} hint={t.osdHint}>
          <Toggle label={t.osd} checked={config.osd.enabled}
            onChange={(enabled) => change({ osd: { enabled } })} />
        </Row>
        <Row label={t.hideDelay}>
          <Stepper label={t.hideDelay} min={500} max={8000} step={500} value={config.osd.hideDelay}
            format={(v) => `${(v / 1000).toFixed(1)} s`} onChange={(hideDelay) => change({ osd: { hideDelay } })} />
        </Row>
        <Row label={t.brightness}>
          <Toggle label={t.brightness} checked={config.osd.enableBrightness}
            onChange={(enableBrightness) => change({ osd: { enableBrightness } })} />
        </Row>
      </Card>
      <Card title={t.session}>
        <Row label={t.enabled} hint={t.sessionHint}>
          <Toggle label={t.session} checked={config.session.enabled}
            onChange={(enabled) => change({ session: { enabled } })} />
        </Row>
        <Row label={t.hotkey}>
          <Select label={t.hotkey} value={config.session.hotkey}
            options={["ctrl+alt+end", "ctrl+alt+home", "none"].map((value) => ({ value, label: hotkeyLabel(value) }))}
            onChange={(hotkey) => change({ session: { hotkey } })} />
        </Row>
      </Card>
    </>,
    look: <>
      <Card title={t.border}>
        <Row label={t.borderThickness}>
          <Slider label={t.borderThickness} min={2} max={40} value={config.border.thickness}
            onChange={(thickness) => change({ border: { thickness } })} />
        </Row>
        <Row label={t.borderRounding}>
          <Slider label={t.borderRounding} min={0} max={60} value={config.border.rounding}
            onChange={(rounding) => change({ border: { rounding } })} />
        </Row>
        <Row label={t.borderSmoothing} hint={t.borderSmoothingHint}>
          <Slider label={t.borderSmoothing} min={0} max={60} value={config.border.smoothing}
            onChange={(smoothing) => change({ border: { smoothing } })} />
        </Row>
      </Card>
      <Card title={t.scale}>
        {([
          ["rounding", t.scaleRounding],
          ["spacing", t.scaleSpacing],
          ["padding", t.scalePadding],
        ] as const).map(([key, label]) => (
          <Row key={key} label={label}>
            <Slider label={label} unit="%" min={50} max={200} value={Math.round(config.appearance[key].scale * 100)}
              onChange={(v) => change({ appearance: { [key]: { scale: v / 100 } } })} />
          </Row>
        ))}
        <Row label={t.scaleFont}>
          <Slider label={t.scaleFont} unit="%" min={75} max={175} value={Math.round(config.appearance.font.size.scale * 100)}
            onChange={(v) => change({ appearance: { font: { size: { scale: v / 100 } } } })} />
        </Row>
      </Card>
      <Card title={t.animations}>
        <Row label={t.animations} hint={t.animationsHint}>
          <Slider label={t.animations} min={0} max={300} value={Math.round(config.appearance.animScale * 100)}
            format={(v) => (v === 0 ? "—" : `${(v / 100).toFixed(1)}×`)}
            onChange={(v) => change({ appearance: { animScale: v / 100 } })} />
        </Row>
      </Card>
      <Card title={t.fonts}>
        <Row label={t.fontSans}>
          <input className="text-input" value={config.appearance.font.family.sans} aria-label={t.fontSans}
            onChange={(e) => change({ appearance: { font: { family: { sans: e.target.value } } } })} />
        </Row>
        <Row label={t.fontMono}>
          <input className="text-input" value={config.appearance.font.family.mono} aria-label={t.fontMono}
            onChange={(e) => change({ appearance: { font: { family: { mono: e.target.value } } } })} />
        </Row>
      </Card>
    </>,
    general: <>
      <Card>
        <Row label={t.language}>
          <Select label={t.language} value={config.general.language}
            options={[{ value: "system", label: t.languageSystem }, ...LANGUAGE_OPTIONS]}
            onChange={(language) => change({ general: { language } })} />
        </Row>
        <Row label={t.autostart}>
          <Toggle label={t.autostart} checked={autostart}
            onChange={(enabled) => {
              setAutostartState(enabled);
              setAutostart(enabled).catch((e) => { setAutostartState(!enabled); setError(String(e)); });
            }} />
        </Row>
        <Row label={t.replaceTaskbar} hint={t.replaceTaskbarHint}>
          <Toggle label={t.replaceTaskbar} checked={config.general.replaceTaskbar}
            onChange={(replaceTaskbar) => change({ general: { replaceTaskbar } })} />
        </Row>
        <Row label={t.reserveSpace} hint={t.reserveSpaceHint}>
          <Toggle label={t.reserveSpace} checked={config.general.reserveSpace}
            onChange={(reserveSpace) => change({ general: { reserveSpace } })} />
        </Row>
      </Card>
      <Card title={t.audio}>
        <Row label={t.maxVolume}>
          <Slider label={t.maxVolume} unit="%" min={10} max={100} value={Math.round(config.services.maxVolume * 100)}
            onChange={(v) => change({ services: { maxVolume: v / 100 } })} />
        </Row>
        <Row label={t.audioStep}>
          <Stepper label={t.audioStep} min={1} max={20} value={Math.round(config.services.audioIncrement * 100)}
            format={(v) => `${v} %`} onChange={(v) => change({ services: { audioIncrement: v / 100 } })} />
        </Row>
        <Row label={t.brightnessStep}>
          <Stepper label={t.brightnessStep} min={1} max={25} value={Math.round(config.services.brightnessIncrement * 100)}
            format={(v) => `${v} %`} onChange={(v) => change({ services: { brightnessIncrement: v / 100 } })} />
        </Row>
      </Card>
      <Card title={t.wallpaperDir}>
        <Row label={t.wallpaperDir} hint={t.wallpaperDirHint} stacked>
          <input className="text-input wide" value={config.paths.wallpaperDir} aria-label={t.wallpaperDir}
            placeholder="C:\Users\…\Pictures\Wallpapers"
            onChange={(e) => change({ paths: { wallpaperDir: e.target.value } })} />
        </Row>
      </Card>
      <Card title={t.updates}>
        <Row label={t.checkUpdates}>
          <Toggle label={t.checkUpdates} checked={config.general.checkUpdates}
            onChange={(checkUpdates) => change({ general: { checkUpdates } })} />
        </Row>
        <SubRow open={config.general.checkUpdates} label={t.autoUpdate}>
          <Toggle label={t.autoUpdate} checked={config.general.autoUpdate}
            onChange={(autoUpdate) => change({ general: { autoUpdate } })} />
        </SubRow>
      </Card>
      <div className="danger-zone">
        <button type="button" className="text-button" onClick={reset}>{t.reset}</button>
      </div>
    </>,
    about: <>
      <Card>
        <div className="about">
          <img className="about-logo" src={norbannerLogo} alt="" />
          <div>
            <h2 className="about-title">Norbanner</h2>
            <p className="about-body">{t.aboutBody}</p>
            <p className="about-version">{t.version} {version}</p>
          </div>
        </div>
        <Row label={t.updates} hint={checking === "checking" ? t.checking : checking === "current" ? t.upToDate : checking === "failed" ? t.checkFailed : undefined}>
          <button type="button" className="tonal-button" disabled={checking === "checking"} onClick={checkForUpdate}>{t.checkNow}</button>
        </Row>
        <Row label={t.repository}>
          <button type="button" className="tonal-button" onClick={() => void openRepository()}>GitHub</button>
        </Row>
      </Card>
    </>,
  };

  return (
    <div className="app" style={paletteStyle(palette)} data-mode={config.theme.mode}>
      <nav className="nav" data-tauri-drag-region>
        <div className="nav-header" data-tauri-drag-region>
          <img className="nav-logo" src={norbannerLogo} alt="" />
          <span className="nav-titles" data-tauri-drag-region>
            <span className="nav-title">{t.title}</span>
            <span className="nav-subtitle">{t.subtitle}</span>
          </span>
        </div>
        {SECTIONS.map((id) => (
          <button key={id} type="button" className="nav-item" aria-current={section === id ? "page" : undefined}
            onClick={() => setSection(id)}>
            <NavIcon section={id} />
            <span>{sectionLabel(t, id)}</span>
          </button>
        ))}
      </nav>
      <main className="content">
        <header className="content-header" data-tauri-drag-region>
          <h1 data-tauri-drag-region>{sectionLabel(t, section)}</h1>
          <span className="window-buttons">
            <button type="button" aria-label="—" onClick={() => void getCurrentWindow().minimize()}>—</button>
            <button type="button" aria-label="✕" onClick={() => void getCurrentWindow().close()}>✕</button>
          </span>
        </header>
        {update && !updateHidden ? (
          <UpdateBanner t={t} info={update} progress={progress} installing={installing} failed={installFailed}
            onInstall={() => {
              setInstalling(true);
              setInstallFailed(false);
              installUpdate().catch(() => { setInstalling(false); setInstallFailed(true); });
            }}
            onLater={() => setUpdateHidden(true)}
            onNotes={() => { if (update.notes) void openReleaseNotes(update.notes); }} />
        ) : null}
        {error ? <p className="error" role="alert">{error}</p> : null}
        <div className="page">{pages[section]}</div>
      </main>
    </div>
  );
}
