import { useEffect, useRef, useState } from "react";
import type { MutableRefObject } from "react";
import { emitTo, listen } from "@tauri-apps/api/event";
import { api, type YearText } from "@/lib/api";
import {
  DISPLAY_EVENT,
  DISPLAY_READY,
  needsSeek,
  type DisplayMessage,
  type PlaybackState,
} from "@/lib/display";
import { useImageUrl } from "@/lib/media";
import { loadLang, loadUiLang } from "@/lib/settings";
import { resolveUiLang, setUiLang, t } from "@/lib/i18n";

type MediaMessage = Extract<DisplayMessage, { type: "media" }>;
type Content = { kind: "idle" } | { kind: "image"; src: string; caption: string } | { kind: "media"; message: MediaMessage };

/** The year text: the quoted scripture and its reference, centered on black. */
function Idle() {
  const [year, setYear] = useState<YearText | null>(null);
  const current = new Date().getFullYear();
  useEffect(() => {
    api.yearText(loadLang(), current).then(setYear, () => undefined);
  }, [current]);
  if (!year) return null;
  return (
    <div className="max-w-[64vw] text-center text-[clamp(1.6rem,4.3vw,5.5rem)] font-semibold leading-snug text-white/80">
      <p>{year.text}</p>
      {year.reference && <p>—{year.reference}.</p>}
    </div>
  );
}

function Picture({ src, caption }: { src: string; caption: string }) {
  const url = useImageUrl(src);
  return url ? <img src={url} alt={caption} className="max-h-full max-w-full object-contain" draggable={false} /> : null;
}

/** Plays what the app's player plays, without controls; the app keeps it in step. */
function Recording({ message, wanted }: { message: MediaMessage; wanted: MutableRefObject<PlaybackState> }) {
  const video = useRef<HTMLVideoElement>(null);
  const poster = useImageUrl(message.poster);

  /** Bring the video to the state the app wants; the app sends it whenever that changes. */
  const apply = () => {
    const el = video.current;
    if (!el) return;
    const s = wanted.current;
    el.playbackRate = s.rate;
    el.volume = s.volume;
    el.muted = s.muted;
    if (needsSeek(el.currentTime, s.time)) el.currentTime = s.time;
    const track = el.textTracks[0];
    if (track) track.mode = s.captions ? "showing" : "disabled";
    if (s.paused && !el.paused) el.pause();
    else if (!s.paused && el.paused) {
      // Without a click in this window the webview may refuse sound; picture beats silence.
      el.play().catch(() => {
        el.muted = true;
        void el.play().catch(() => undefined);
      });
    }
  };
  useEffect(apply);

  return (
    <>
      {message.kind === "audio" && poster && (
        <img src={poster} alt="" className="absolute max-h-[70%] max-w-[70%] object-contain" />
      )}
      <video
        key={message.url}
        ref={video}
        src={message.url}
        autoPlay
        playsInline
        onLoadedMetadata={apply}
        className={message.kind === "audio" ? "h-full w-full opacity-0" : "h-full w-full bg-black object-contain"}
      >
        {message.subtitles && <track kind="subtitles" src={message.subtitles} label={t("Subtitles")} />}
      </video>
    </>
  );
}

/** The second display: black, the year text until something is shown. */
export function DisplayView() {
  const [content, setContent] = useState<Content>({ kind: "idle" });
  const wanted = useRef<PlaybackState>({ time: 0, paused: false, rate: 1, volume: 1, muted: false, captions: false });
  const [, tick] = useState(0);

  useEffect(() => {
    setUiLang(resolveUiLang(loadUiLang(), navigator.language));
    const un = listen<DisplayMessage>(DISPLAY_EVENT, ({ payload: m }) => {
      if (m.type === "idle") setContent({ kind: "idle" });
      else if (m.type === "image") setContent({ kind: "image", src: m.src, caption: m.caption });
      else {
        wanted.current = { time: m.time, paused: m.paused, rate: m.rate, volume: m.volume, muted: m.muted, captions: m.captions };
        if (m.type === "media") setContent({ kind: "media", message: m });
        else tick((n) => n + 1);
      }
    });
    // Ask for what is showing now, in case the app sent it before this window listened.
    void un.then(() => emitTo("main", DISPLAY_READY));
    return () => void un.then((f) => f());
  }, []);

  const toggleFullscreen = () => {
    if (document.fullscreenElement) void document.exitFullscreen();
    else void document.documentElement.requestFullscreen().catch(() => undefined);
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F11" || e.key.toLowerCase() === "f") {
        e.preventDefault();
        toggleFullscreen();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div
      onDoubleClick={toggleFullscreen}
      className="fixed inset-0 flex cursor-none items-center justify-center overflow-hidden bg-black text-white"
    >
      {content.kind === "idle" && <Idle />}
      {content.kind === "image" && <Picture src={content.src} caption={content.caption} />}
      {content.kind === "media" && <Recording message={content.message} wanted={wanted} />}
    </div>
  );
}
