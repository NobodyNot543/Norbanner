// Small offline previews. Full-resolution originals are embedded only in Settings native code.
import preview1 from "./assets/wallpapers/desktop-01.jpg";
import preview2 from "./assets/wallpapers/desktop-02.jpg";
import preview3 from "./assets/wallpapers/desktop-03.jpg";
import preview4 from "./assets/wallpapers/desktop-04.jpg";
import preview5 from "./assets/wallpapers/desktop-05.jpg";
import preview6 from "./assets/wallpapers/desktop-06.jpg";

export const WALLPAPERS = [
  { id: 1, preview: preview1 },
  { id: 2, preview: preview2 },
  { id: 3, preview: preview3 },
  { id: 4, preview: preview4 },
  { id: 5, preview: preview5 },
  { id: 6, preview: preview6 },
] as const;
