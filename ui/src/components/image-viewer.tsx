import { useEffect, useRef, useState } from "react";
import { RotateCcw, X, ZoomIn, ZoomOut } from "lucide-react";
import { useApp } from "@/app";
import { useImageUrl } from "@/lib/media";
import { t } from "@/lib/i18n";

const MAX_ZOOM = 6;
const iconButton = "flex h-8 w-8 items-center justify-center rounded-md hover:bg-bar disabled:opacity-40";

/** A clicked picture in the content area, like the player; also shown on the second display. */
export function ImageViewer() {
  const { viewer } = useApp();
  return viewer ? <Viewer key={viewer.src} src={viewer.src} caption={viewer.caption} /> : null;
}

function Viewer({ src, caption }: { src: string; caption: string }) {
  const { view, closeImage } = useApp();
  const url = useImageUrl(src);
  const [zoom, setZoom] = useState(1);
  const zoomBy = (factor: number) => setZoom((z) => Math.min(MAX_ZOOM, Math.max(1, z * factor)));

  // Going somewhere else in the app closes the picture.
  const shownView = useRef(view);
  useEffect(() => {
    if (shownView.current !== view) closeImage();
  }, [view, closeImage]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeImage();
      else if (e.key === "+" || e.key === "=") zoomBy(1.25);
      else if (e.key === "-") zoomBy(1 / 1.25);
      else if (e.key === "0") setZoom(1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [closeImage]);

  return (
    <div role="dialog" aria-label={caption || t("Image")} className="absolute inset-0 z-40 flex flex-col bg-surface text-fg">
      <header className="flex shrink-0 items-center gap-2 border-b border-line bg-surface px-2 py-2">
        <h2 className="min-w-0 flex-1 truncate px-2 text-[1rem] font-medium">{caption || t("Image")}</h2>
        <button aria-label={t("Zoom out")} title={t("Zoom out")} className={iconButton} disabled={zoom <= 1} onClick={() => zoomBy(1 / 1.25)}>
          <ZoomOut size={19} />
        </button>
        <button aria-label={t("Zoom in")} title={t("Zoom in")} className={iconButton} disabled={zoom >= MAX_ZOOM} onClick={() => zoomBy(1.25)}>
          <ZoomIn size={19} />
        </button>
        <button aria-label={t("Reset zoom")} title={t("Reset zoom")} className={iconButton} disabled={zoom === 1} onClick={() => setZoom(1)}>
          <RotateCcw size={18} />
        </button>
        <button aria-label={t("Close")} title={t("Close")} className={iconButton} onClick={closeImage}>
          <X size={20} />
        </button>
      </header>
      <div
        className="flex min-h-0 flex-1 overflow-auto"
        onWheel={(e) => zoomBy(e.deltaY < 0 ? 1.1 : 1 / 1.1)}
        onDoubleClick={() => setZoom((z) => (z > 1 ? 1 : 2.5))}
      >
        {url && (
          <img
            src={url}
            alt={caption}
            draggable={false}
            className="m-auto select-none object-contain"
            style={zoom === 1 ? { maxWidth: "100%", maxHeight: "100%" } : { width: `${zoom * 100}%`, maxWidth: "none" }}
          />
        )}
      </div>
      {caption && <p className="shrink-0 border-t border-line bg-surface px-4 py-2 text-sm text-muted">{caption}</p>}
    </div>
  );
}
