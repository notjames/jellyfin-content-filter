export interface NativeSignal<T extends unknown[] = []> {
  connect(callback: (...args: T) => void): void;
  disconnect(callback: (...args: T) => void): void;
}

export interface NativePlayer {
  getPosition(callback?: (positionMs: number) => void): number | void;
  seekTo(positionMs: number): void;
  setMuted(muted: boolean): void;
  muted?(callback?: (muted: boolean) => void): boolean | void;
  setSubtitleStream?(index: number): void;
  positionUpdate?: NativeSignal<[number]>;
  finished?: NativeSignal;
  canceled?: NativeSignal;
  stopped?: NativeSignal;
}

interface DesktopApi {
  player?: NativePlayer;
}

interface DesktopVideoPlayer {
  isMuted?: () => boolean;
  _currentPlayOptions?: {
    itemId?: string;
    ItemId?: string;
    item?: { Id?: string; id?: string };
    mediaSource?: { ItemId?: string; itemId?: string };
  };
}

declare global {
  interface Window {
    api?: DesktopApi;
    apiPromise?: Promise<DesktopApi>;
    jmpInfo?: { mode?: string };
    jmpNative?: { playerSeek?: (positionMs: number) => void };
    NativeShell?: unknown;
    _mpvVideoPlayerInstance?: DesktopVideoPlayer;
  }
}

export function isDesktopClient(): boolean {
  return Boolean(
    window.jmpInfo?.mode === "desktop"
      || window.apiPromise
      || window.api?.player
      || window.jmpNative
      || window.NativeShell
  );
}

export async function resolveNativePlayer(timeoutMs = 10_000): Promise<NativePlayer> {
  const direct = window.api?.player;
  if (direct) return direct;

  if (window.apiPromise) {
    const api = await window.apiPromise;
    if (api.player) return api.player;
  }

  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    if (window.api?.player) return window.api.player;
    await new Promise((resolve) => window.setTimeout(resolve, 50));
  }
  throw new Error("Jellyfin Desktop native player bridge was not found");
}

export async function readNativeMute(player: NativePlayer): Promise<boolean | undefined> {
  const desktopValue = window._mpvVideoPlayerInstance?.isMuted?.();
  if (typeof desktopValue === "boolean") return desktopValue;
  if (!player.muted) return undefined;

  return new Promise((resolve) => {
    let settled = false;
    const finish = (value: boolean | undefined) => {
      if (settled) return;
      settled = true;
      resolve(value);
    };
    const returned = player.muted?.((value) => finish(Boolean(value)));
    if (typeof returned === "boolean") finish(returned);
    window.setTimeout(() => finish(undefined), 100);
  });
}
