import { useCallback, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import {
  Captions,
  ChevronDown,
  ExternalLink,
  Maximize,
  Minimize,
  Pause,
  Play,
  Radio,
  Repeat,
  RotateCcw,
  RotateCw,
  Settings,
  SkipBack,
  Volume2,
  VolumeX,
  X,
} from "lucide-react";
import { useApp } from "@/app";
import { api } from "@/lib/api";
import { isMediaUrl, mediaBlobUrl } from "@/lib/media";
import { SPEEDS, formatDuration, pickSource, sizeMb, type PlayerItem, type PlayerSource } from "@/lib/player";
import {
  AUTO_CEILING,
  HEALTHY_AHEAD,
  HEALTHY_FOR_MS,
  autoStart,
  stepDown,
  stepUp,
  tooManyStalls,
} from "@/lib/quality";
import { loadAutoQuality, loadMediaQuality, saveAutoQuality, saveMediaQuality } from "@/lib/settings";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";

/** Height of the window in device pixels, which is what a recording can use at most. */
const windowHeight = () => Math.round(window.innerHeight * (window.devicePixelRatio || 1));

/** Blob URL for a `jwmedia:` image; other URLs are used as they are. */
function useImageUrl(src: string | null): string | undefined {
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

const iconButton = "flex h-8 w-8 items-center justify-center hover:bg-white/15";
const chip = "flex items-center gap-1 px-2 py-1 hover:bg-white/15 aria-pressed:bg-white/20 aria-pressed:text-accent";
const select = "bg-white/10 px-2 py-1 text-white [&>option]:text-black";

function Control({ label, onClick, children }: { label: string; onClick: () => void; children: ReactNode }) {
  return (
    <button aria-label={label} title={label} className={iconButton} onClick={onClick}>
      {children}
    </button>
  );
}

/**
 * The recording being played. It lives in the content area next to the rail
 * and keeps playing when the user navigates: minimised it is a bar at the
 * bottom, expanded it fills the content area. The `<video>` element stays
 * mounted when switching, so playback is not interrupted.
 */
export function PlayerHost() {
  const { player } = useApp();
  return player ? <Player key={player.id} item={player.item} /> : null;
}

function Player({ item }: { item: PlayerItem }) {
  const { lang, view, closePlayer } = useApp();
  const [full, setFull] = useState(true);
  const [screen, setScreen] = useState(false);
  const [auto, setAuto] = useState(loadAutoQuality);
  const [source, setSource] = useState<PlayerSource | undefined>(() =>
    loadAutoQuality() ? autoStart(item.sources, windowHeight()) : pickSource(item.sources, loadMediaQuality()),
  );
  const [speed, setSpeed] = useState(1);
  const [repeat, setRepeat] = useState(false);
  const [captions, setCaptions] = useState(false);
  const [menu, setMenu] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [paused, setPaused] = useState(true);
  const [time, setTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(1);
  const [muted, setMuted] = useState(false);
  const media = useRef<HTMLVideoElement>(null);
  const root = useRef<HTMLDivElement>(null);
  /** Where to continue after switching quality. */
  const resume = useRef(0);
  /** When playback ran dry, and the qualities that did not keep up (never tried again). */
  const stalls = useRef<number[]>([]);
  const blocked = useRef(new Set<string>());
  const healthySince = useRef<number | null>(null);
  const poster = useImageUrl(item.poster);

  // Going somewhere else in the app shrinks the player to its bar.
  const shownView = useRef(view);
  useEffect(() => {
    if (shownView.current !== view) setFull(false);
    shownView.current = view;
  }, [view]);

  const applyCaptions = (on: boolean) => {
    const track = media.current?.textTracks[0];
    if (track) track.mode = on ? "showing" : "disabled";
  };
  useEffect(() => applyCaptions(captions), [captions]);

  const toggle = useCallback(() => {
    const el = media.current;
    if (!el) return;
    if (el.paused) void el.play().catch(() => undefined);
    else el.pause();
  }, []);
  const seekBy = useCallback((delta: number) => {
    const el = media.current;
    if (el) el.currentTime = Math.max(0, Math.min(el.duration || Infinity, el.currentTime + delta));
  }, []);

  const toggleScreen = () => {
    if (document.fullscreenElement) void document.exitFullscreen();
    else void root.current?.requestFullscreen().catch(() => undefined);
  };

  useEffect(() => {
    const change = () => setScreen(document.fullscreenElement === root.current);
    document.addEventListener("fullscreenchange", change);
    return () => {
      document.removeEventListener("fullscreenchange", change);
      if (document.fullscreenElement) void document.exitFullscreen();
    };
  }, []);

  // Keys work while the recording fills the content area and nothing is being typed.
  useEffect(() => {
    if (!full) return;
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest("input, select, textarea")) return;
      if (e.key === "Escape") {
        if (menu) setMenu(false);
        else if (!document.fullscreenElement) setFull(false);
      } else if (e.key === " ") {
        e.preventDefault();
        toggle();
      } else if (e.key === "ArrowLeft") seekBy(-5);
      else if (e.key === "ArrowRight") seekBy(15);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [full, menu, toggle, seekBy]);

  const onLoaded = () => {
    const el = media.current;
    if (!el) return;
    setDuration(el.duration || 0);
    el.playbackRate = speed;
    if (resume.current > 0) el.currentTime = resume.current;
    resume.current = 0;
    applyCaptions(captions);
  };
  const switchTo = (s: PlayerSource) => {
    resume.current = media.current?.currentTime ?? 0;
    stalls.current = [];
    healthySince.current = null;
    setError(null);
    setSource(s);
  };
  /** The user picked a fixed quality. */
  const choose = (s: PlayerSource) => {
    const height = parseInt(s.label, 10);
    if (height) saveMediaQuality(height);
    setAuto(false);
    switchTo(s);
  };
  const chooseAuto = () => {
    saveAutoQuality();
    setAuto(true);
    blocked.current.clear();
    const s = autoStart(item.sources, windowHeight());
    if (s && s.url !== source?.url) switchTo(s);
  };
  /** Playback ran dry: after two stalls in a short time, drop to the next smaller quality. */
  const onWaiting = (el: HTMLVideoElement) => {
    if (!auto || !source || el.paused || el.seeking || el.currentTime < 1) return;
    stalls.current = [...stalls.current, Date.now()];
    if (!tooManyStalls(stalls.current, Date.now())) return;
    const lower = stepDown(item.sources, source);
    if (lower) {
      blocked.current.add(source.url);
      switchTo(lower);
    }
  };
  /** A buffer that stays full means the connection has room for the next quality. */
  const checkBuffer = (el: HTMLVideoElement) => {
    if (!auto || !source || el.paused || el.seeking) return void (healthySince.current = null);
    let ahead = 0;
    for (let i = 0; i < el.buffered.length; i++) {
      if (el.buffered.start(i) <= el.currentTime && el.currentTime <= el.buffered.end(i)) {
        ahead = el.buffered.end(i) - el.currentTime;
      }
    }
    if (ahead < HEALTHY_AHEAD) return void (healthySince.current = null);
    const now = Date.now();
    healthySince.current ??= now;
    if (now - healthySince.current < HEALTHY_FOR_MS) return;
    const up = stepUp(item.sources, source, Math.min(AUTO_CEILING, windowHeight()), blocked.current);
    if (up) switchTo(up);
    else healthySince.current = null;
  };
  const changeSpeed = (v: number) => {
    setSpeed(v);
    if (media.current) media.current.playbackRate = v;
  };
  const changeVolume = (v: number) => {
    setVolume(v);
    setMuted(v === 0);
    if (media.current) {
      media.current.volume = v;
      media.current.muted = v === 0;
    }
  };
  const toggleMute = () => {
    const next = !muted;
    setMuted(next);
    if (media.current) media.current.muted = next;
  };
  const onPlayError = () => setError(t("This system cannot play the recording here. Open it in your browser instead."));
  const remote = source?.url.startsWith("https://") ?? false;
  const label = `${remote ? t("Streaming") : t("Playing")} · ${item.title}`;

  const settings = (
    <div
      role="dialog"
      aria-label={t("Settings")}
      className="absolute bottom-full right-2 z-10 mb-1 flex min-w-[240px] flex-col gap-3 bg-[#2b2b2b] p-4 text-sm shadow-xl ring-1 ring-white/15"
    >
      {item.sources.length > 1 && (
        <label className="flex items-center justify-between gap-3">
          {t("Quality")}
          <select
            value={auto ? "auto" : (source?.url ?? "")}
            onChange={(e) =>
              e.target.value === "auto" ? chooseAuto() : choose(item.sources.find((s) => s.url === e.target.value)!)
            }
            className={select}
          >
            <option value="auto">{[t("Auto"), auto ? source?.label : ""].filter(Boolean).join(" · ")}</option>
            {item.sources.map((s) => (
              <option key={s.url} value={s.url}>
                {[s.label, s.size ? sizeMb(s.size) : ""].filter(Boolean).join(" · ")}
              </option>
            ))}
          </select>
        </label>
      )}
      <label className="flex items-center justify-between gap-3">
        {t("Speed")}
        <select value={speed} onChange={(e) => changeSpeed(Number(e.target.value))} className={select}>
          {SPEEDS.map((v) => (
            <option key={v} value={v}>
              {v === 1 ? t("Normal") : `${v}×`}
            </option>
          ))}
        </select>
      </label>
      <div className="flex flex-wrap gap-1">
        <button aria-pressed={repeat} className={chip} onClick={() => setRepeat((r) => !r)}>
          <Repeat size={16} />
          {t("Repeat")}
        </button>
        {source?.subtitles && (
          <button aria-pressed={captions} className={chip} onClick={() => setCaptions((c) => !c)}>
            <Captions size={16} />
            {t("Subtitles")}
          </button>
        )}
        {remote && source && (
          <button className={chip} onClick={() => void api.openExternal(source.url)}>
            <ExternalLink size={16} />
            {t("Open in browser")}
          </button>
        )}
      </div>
    </div>
  );

  const controls = (
    <div className="relative shrink-0 bg-[#1f1f1f] px-3 pb-2 pt-1">
      {menu && settings}
      <input
        type="range"
        aria-label={t("Position")}
        min={0}
        max={duration || 0}
        step={0.1}
        value={Math.min(time, duration || 0)}
        onChange={(e) => {
          const v = Number(e.target.value);
          setTime(v);
          if (media.current) media.current.currentTime = v;
        }}
        className="block h-3 w-full cursor-pointer accent-accent"
      />
      <div className="mt-1 flex items-center justify-between gap-3 text-[0.8rem]">
        {full ? (
          <span className="flex min-w-0 items-center gap-2 text-white/80">
            <Radio size={14} className="shrink-0" />
            <span className="truncate">{label}</span>
          </span>
        ) : (
          <button
            className="flex min-w-0 items-center gap-2 text-left text-white/80 hover:text-white"
            title={t("Expand")}
            onClick={() => setFull(true)}
          >
            <Radio size={14} className="shrink-0" />
            <span className="truncate">{label}</span>
          </button>
        )}
        <span className="shrink-0 tabular-nums text-white/80">
          {formatDuration(time)}/{formatDuration(duration)}
        </span>
      </div>
      {error && (
        <p role="alert" className="mt-1 truncate text-[0.8rem] text-white" title={error}>
          {error}
        </p>
      )}
      <div className="mt-1 flex items-center">
        <Control label={paused ? t("Play") : t("Pause")} onClick={toggle}>
          {paused ? <Play size={20} /> : <Pause size={20} />}
        </Control>
        <Control label={t("Back 5 seconds")} onClick={() => seekBy(-5)}>
          <RotateCcw size={18} />
        </Control>
        <Control label={t("Forward 15 seconds")} onClick={() => seekBy(15)}>
          <RotateCw size={18} />
        </Control>
        <Control label={t("Restart")} onClick={() => seekBy(-Infinity)}>
          <SkipBack size={18} />
        </Control>
        <Control label={muted ? t("Unmute") : t("Mute")} onClick={toggleMute}>
          {muted || volume === 0 ? <VolumeX size={18} /> : <Volume2 size={18} />}
        </Control>
        <input
          type="range"
          aria-label={t("Volume")}
          min={0}
          max={1}
          step={0.05}
          value={muted ? 0 : volume}
          onChange={(e) => changeVolume(Number(e.target.value))}
          className="ml-1 hidden h-3 w-24 cursor-pointer accent-accent sm:block"
        />
        <span className="ml-auto flex items-center">
          <button
            aria-label={t("Settings")}
            title={t("Settings")}
            aria-expanded={menu}
            className={cn(iconButton, menu && "bg-white/15")}
            onClick={() => setMenu((m) => !m)}
          >
            <Settings size={19} />
          </button>
          {full ? (
            <Control label={screen ? t("Exit full screen") : t("Full screen")} onClick={toggleScreen}>
              {screen ? <Minimize size={19} /> : <Maximize size={19} />}
            </Control>
          ) : (
            <Control label={t("Close")} onClick={closePlayer}>
              <X size={20} />
            </Control>
          )}
        </span>
      </div>
    </div>
  );

  return (
    <div
      ref={root}
      role={full ? "dialog" : "region"}
      aria-label={item.title}
      className={cn("text-white", full ? "absolute inset-0 z-30 flex flex-col bg-black" : "shrink-0")}
    >
      {full && (
        <header key="header" className="flex shrink-0 items-center gap-2 bg-[#1f1f1f] px-2 py-2">
          <Control label={t("Minimize")} onClick={() => setFull(false)}>
            <ChevronDown size={22} />
          </Control>
          <h2 className="min-w-0 flex-1 truncate text-[1rem] font-medium">{item.title}</h2>
          <Control label={t("Close")} onClick={closePlayer}>
            <X size={20} />
          </Control>
        </header>
      )}
      <div
        key="stage"
        className={cn(
          full
            ? "relative flex min-h-0 flex-1 items-center justify-center"
            : "pointer-events-none absolute h-px w-px overflow-hidden opacity-0",
        )}
      >
        {item.kind === "audio" && poster && (
          <img src={poster} alt="" className="absolute max-h-[60%] max-w-[min(80%,420px)] object-contain" />
        )}
        <video
          key={source?.url}
          ref={media}
          src={source?.url}
          poster={item.kind === "video" ? poster : undefined}
          autoPlay
          loop={repeat}
          onClick={toggle}
          onLoadedMetadata={onLoaded}
          onTimeUpdate={(e) => {
            setTime(e.currentTarget.currentTime);
            checkBuffer(e.currentTarget);
          }}
          onWaiting={(e) => onWaiting(e.currentTarget)}
          onDurationChange={(e) => setDuration(e.currentTarget.duration || 0)}
          onPlay={() => setPaused(false)}
          onPause={() => setPaused(true)}
          onError={onPlayError}
          className={cn("h-full w-full bg-black object-contain", item.kind === "audio" && "opacity-0")}
        >
          {source?.subtitles && (
            <track kind="subtitles" src={source.subtitles} srcLang={lang.toLowerCase()} label={t("Subtitles")} />
          )}
        </video>
      </div>
      <div key="controls">{controls}</div>
    </div>
  );
}
