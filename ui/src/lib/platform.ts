declare global {
  interface Window {
    __PERGAMENT_PLATFORM__?: string;
    __PERGAMENT_DEVICE__?: string;
  }
}

/** Set by the iOS shell's shim (tauri-shim.js, WebShell.swift); absent on desktop. */
export const isIOS = window.__PERGAMENT_PLATFORM__ === "ios";
export const isIPad = isIOS && window.__PERGAMENT_DEVICE__ === "ipad";

/** iPhone has one window only. The iPad path is unverified until tried on a device running a multi-window system. */
export const canOpenNoteWindow = () => !isIOS || isIPad;

/** Whether the reader makes its own selection: on iOS the system's selection menu would cover it. */
export const ownSelection = () => window.__PERGAMENT_PLATFORM__ === "ios";
