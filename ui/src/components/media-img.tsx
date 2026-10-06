import { useEffect, useRef, useState } from "react";
import type { ImgHTMLAttributes } from "react";
import { isMediaUrl, mediaBlobUrl } from "@/lib/media";

/** An `<img>` that also takes `jwmedia:` URLs, loaded once it scrolls into view. */
export function MediaImg({ src, onError, ...rest }: ImgHTMLAttributes<HTMLImageElement>) {
  const ref = useRef<HTMLImageElement>(null);
  const [loaded, setLoaded] = useState<string | null>(null);
  const [visible, setVisible] = useState(false);
  const media = isMediaUrl(src);

  useEffect(() => {
    const el = ref.current;
    if (!media || !el) return;
    const io = new IntersectionObserver((entries) => entries.some((e) => e.isIntersecting) && setVisible(true), {
      rootMargin: "200px",
    });
    io.observe(el);
    return () => io.disconnect();
  }, [media]);

  useEffect(() => {
    if (!media || !visible || !src) return;
    let live = true;
    setLoaded(null);
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
