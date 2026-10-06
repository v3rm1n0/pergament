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

export const isMediaUrl = (src: string | null | undefined): src is string => !!src && src.startsWith("jwmedia:");

/**
 * Blob URL for a `jwmedia:` image. WebKit refuses the custom scheme from the
 * dev server's origin, so images are fetched through a command instead.
 */
export function mediaBlobUrl(src: string): Promise<string> {
  let blob = blobs.get(src);
  if (!blob) {
    const ext = src.split(".").pop()?.toLowerCase() ?? "";
    blob = api.media(src).then((bytes) => URL.createObjectURL(new Blob([bytes], { type: TYPES[ext] })));
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
    img.removeAttribute("src");
    mediaBlobUrl(src)
      .then((url) => img.isConnected && img.setAttribute("src", url))
      .catch(() => undefined);
  });
}
