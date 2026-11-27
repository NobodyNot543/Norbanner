import { useEffect, useRef, useState } from "react";
import { Card } from "./controls";
import { applyWallpaper, applyWallpaperFile, loadWallpaperPreview, loadWallpaperSelection, pickImage } from "./config";
import { WALLPAPERS } from "./wallpaper-catalog";

interface WallpaperTexts {
  wallpaperGalleryTitle: string;
  wallpaperGalleryHint: string;
  wallpaperApplyLabel: string;
  wallpaperApplying: string;
  wallpaperApplied: string;
  wallpaperApplyError: string;
  wallpaperNames: readonly string[];
  wallpaperFromPc: string;
  wallpaperFromPcLabel: string;
  logoImages: string;
}

/** Marks the computer's own image tile while it is being applied. */
const FROM_PC = 0;

/** Mounted only in Appearance. No polling, full-size image decoding or remote URLs. */
export function WallpaperGallery({ texts, onApplied }: { texts: WallpaperTexts; onApplied: (preview: string) => void }) {
  const [selected, setSelected] = useState<number | null>(null);
  const [pending, setPending] = useState<number | null>(null);
  const [failed, setFailed] = useState(false);
  const [applied, setApplied] = useState(false);
  const busy = useRef(false);
  const revision = useRef(0);
  const mounted = useRef(true);

  useEffect(() => {
    let alive = true;
    mounted.current = true;
    const current = revision.current;
    loadWallpaperSelection().then((id) => {
      if (alive && revision.current === current) setSelected(id);
    }).catch(() => { /* An unavailable read must not prevent an explicit choice. */ });
    return () => { alive = false; mounted.current = false; };
  }, []);

  const choose = async (id: number, preview: string) => {
    // A ref closes the gap before React paints the disabled buttons.
    if (busy.current) return;
    busy.current = true;
    revision.current += 1;
    setPending(id);
    setFailed(false);
    setApplied(false);
    try {
      await applyWallpaper(id, preview);
      // Keep other Settings previews current even if the user has navigated away.
      onApplied(preview);
      if (mounted.current) { setSelected(id); setApplied(true); }
    } catch {
      if (mounted.current) setFailed(true);
    } finally {
      busy.current = false;
      if (mounted.current) setPending(null);
    }
  };

  /** An image from the computer: Windows' dialog, then the same flow. */
  const chooseFromPc = async () => {
    if (busy.current) return;
    const path = await pickImage(texts.logoImages).catch(() => null);
    if (!path) return;
    busy.current = true;
    revision.current += 1;
    setPending(FROM_PC);
    setFailed(false);
    setApplied(false);
    try {
      await applyWallpaperFile(path);
      const preview = await loadWallpaperPreview();
      if (preview) onApplied(preview);
      if (mounted.current) { setSelected(null); setApplied(true); }
    } catch {
      if (mounted.current) setFailed(true);
    } finally {
      busy.current = false;
      if (mounted.current) setPending(null);
    }
  };

  return <Card title={texts.wallpaperGalleryTitle}>
    <p className="wallpaper-hint">{texts.wallpaperGalleryHint}</p>
    <div className="wallpaper-grid">
      {WALLPAPERS.map(({ id, preview }) => {
        const name = texts.wallpaperNames[id - 1];
        return <button type="button" className="wallpaper-choice" key={id}
          data-wallpaper-id={id} title={name}
          aria-label={texts.wallpaperApplyLabel.replace("{name}", name)}
          aria-pressed={selected === id} aria-busy={pending === id || undefined}
          disabled={pending !== null} onClick={() => choose(id, preview)}>
          <span className="wallpaper-image">
            <img src={preview} width="400" height="225" loading="lazy" decoding="async" alt="" />
            {selected === id ? <span className="wallpaper-check" aria-hidden="true">
              <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2"><path d="m4 8 2.5 2.5L12 5" /></svg>
            </span> : null}
            <span className="wallpaper-name" aria-hidden="true">{name}</span>
          </span>
        </button>;
      })}
      <button type="button" className="wallpaper-choice wallpaper-choice-wide" title={texts.wallpaperFromPcLabel}
        aria-label={texts.wallpaperFromPcLabel} aria-busy={pending === FROM_PC || undefined}
        disabled={pending !== null} onClick={() => void chooseFromPc()}>
        <span className="wallpaper-image wallpaper-pick">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <rect x="3.5" y="4.5" width="17" height="15" rx="3" /><circle cx="9" cy="10" r="1.6" /><path d="m4 17 5-4.5 4 3.5 2.5-2 4.5 3.5" />
          </svg>
          <span className="wallpaper-pick-label">{texts.wallpaperFromPc}</span>
        </span>
      </button>
    </div>
    <p className="wallpaper-status" role={failed ? "alert" : "status"}>
      {pending !== null ? texts.wallpaperApplying : failed ? texts.wallpaperApplyError : applied ? texts.wallpaperApplied : ""}
    </p>
  </Card>;
}
