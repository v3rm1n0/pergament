import { useEffect, useRef, useState } from "react";
import type { ImgHTMLAttributes } from "react";
import { cachedBlobUrl, isMediaUrl, mediaBlobUrl } from "@/lib/media";

/** An `<img>` that also takes `jwmedia:` URLs, loaded once it scrolls into view. */
export function MediaImg({ src, onError, ...rest }: ImgHTMLAttributes<HTMLImageElement>) {
  const ref = useRef<HTMLImageElement>(null);
  // An image loaded before is shown on the first render, so remounting does not flash an empty tile.
  const [loaded, setLoaded] = useState<string | null>(() => cachedBlobUrl(src) ?? null);
  const [visible, setVisible] = useState(false);
  const media = isMediaUrl(src);

  useEffect(() => {
    const el = ref.current;
    if (!media || !el || loaded) return;
    const io = new IntersectionObserver((entries) => entries.some((e) => e.isIntersecting) && setVisible(true), {
      rootMargin: "200px",
    });
    io.observe(el);
    return () => io.disconnect();
  }, [media, loaded]);

  useEffect(() => {
    if (!media || !src) return;
    const cached = cachedBlobUrl(src);
    if (cached) return setLoaded(cached);
    if (!visible) return;
    let live = true;
    mediaBlobUrl(src)
      .then((url) => live && setLoaded(url))
      .catch(() => live && onError?.({} as never));
    return () => {
      live = false;
    };
    // onError is only called when loading fails.
  }, [media, visible, src]);

  return <img ref={ref} src={media ? (loaded ?? undefined) : src} onError={onError} {...rest} />;
}
