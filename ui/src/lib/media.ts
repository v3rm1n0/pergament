import { useEffect, useState } from "react";
import { api } from "@/lib/api";

const TYPES: Record<string, string> = {
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  png: "image/png",
  gif: "image/gif",
  svg: "image/svg+xml",
  webp: "image/webp",
};

const blobs = new Map<string, Promise<string>>();
const resolved = new Map<string, string>();

/** Blob URL of a `jwmedia:` image that was loaded before, available without waiting. */
export const cachedBlobUrl = (src: string | null | undefined): string | undefined =>
  isMediaUrl(src) ? resolved.get(src) : undefined;

export const isMediaUrl = (src: string | null | undefined): src is string => !!src && src.startsWith("jwmedia:");

/**
 * Blob URL for a `jwmedia:` image. WebKit refuses the custom scheme from the
 * dev server's origin, so images are fetched through a command instead.
 */
export function mediaBlobUrl(src: string): Promise<string> {
  let blob = blobs.get(src);
  if (!blob) {
    const ext = src.split(".").pop()?.toLowerCase() ?? "";
    blob = api.media(src).then((bytes) => {
      const url = URL.createObjectURL(new Blob([bytes], { type: TYPES[ext] }));
      resolved.set(src, url);
      return url;
    });
    blob.catch(() => blobs.delete(src));
    blobs.set(src, blob);
  }
  return blob;
}

/** Loads the `jwmedia:` images inside rendered page HTML. */
export function hydrateMedia(root: ParentNode) {
  root.querySelectorAll<HTMLImageElement>("img").forEach((img) => {
    const src = img.getAttribute("src");
    if (!isMediaUrl(src)) return;
    // The original URL is what the second display loads.
    img.dataset.jwmedia = src;
    img.removeAttribute("src");
    mediaBlobUrl(src)
      .then((url) => img.isConnected && img.setAttribute("src", url))
      .catch(() => undefined);
  });
}

/** An image of a page that was clicked: its `jwmedia:` URL and caption. Small pictures, such as icons, do not count. */
export function clickedImage(target: EventTarget | null): { src: string; caption: string } | null {
  const img = (target as HTMLElement | null)?.closest<HTMLImageElement>("img[data-jwmedia]");
  if (!img || img.closest("a") || img.width < 100) return null;
  const caption = img.closest("figure")?.querySelector("figcaption")?.textContent?.trim() || img.alt.trim();
  return { src: img.dataset.jwmedia!, caption };
}

/** Blob URL for a `jwmedia:` image; other URLs are used as they are. */
export function useImageUrl(src: string | null): string | undefined {
  const [url, setUrl] = useState<string>();
  useEffect(() => {
    if (!src) return setUrl(undefined);
    if (!isMediaUrl(src)) return setUrl(src);
    let live = true;
    mediaBlobUrl(src)
      .then((u) => live && setUrl(u))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [src]);
  return url;
}
