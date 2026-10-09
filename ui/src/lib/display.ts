import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

/** Label of the second display window (DISPLAY in src-tauri/src/lib.rs). */
export const DISPLAY_LABEL = "display";
/** Events: the app sends `DISPLAY_EVENT` to the display, the display answers with the other two. */
export const DISPLAY_EVENT = "display-message";
export const DISPLAY_READY = "display-ready";
export const DISPLAY_CLOSED = "display-closed";

/** How far playback on the display may drift from the app before it is corrected. */
export const DRIFT_SECONDS = 0.7;

/** Playback state the display copies from the app's player. */
export interface PlaybackState {
  time: number;
  paused: boolean;
  rate: number;
  volume: number;
  muted: boolean;
  /** Show the subtitles of the recording. */
  captions: boolean;
}

/** What the second display shows. */
export type DisplayMessage =
  /** The year text, on black. */
  | { type: "idle" }
  | { type: "image"; /** `jwmedia:` URL */ src: string; caption: string }
  | ({
      type: "media";
      url: string;
      kind: "audio" | "video";
      poster: string | null;
      subtitles: string | null;
    } & PlaybackState)
  | ({ type: "media-state" } & PlaybackState);

export const isDisplayWindow = () => getCurrentWebviewWindow().label === DISPLAY_LABEL;

export const sendToDisplay = (message: DisplayMessage) =>
  emitTo(DISPLAY_LABEL, DISPLAY_EVENT, message).catch(() => undefined);

/** Whether the display's playback position has drifted too far and needs a seek. */
export const needsSeek = (shown: number, wanted: number) => Math.abs(shown - wanted) > DRIFT_SECONDS;

/** The last message to replay when the display window has just opened. */
export function remember(last: DisplayMessage, message: DisplayMessage): DisplayMessage {
  if (message.type !== "media-state") return message;
  if (last.type !== "media") return last;
  const { type: _, ...state } = message;
  return { ...last, ...state };
}
