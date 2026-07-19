import { sessionFromPlaybackRequest, type Session } from "./api";

export interface PlaybackTarget {
  itemId: string;
  session: Session;
}

const PLAYBACK_PATH = /\/Items\/([^/]+)\/PlaybackInfo\/?$/i;

export class PlaybackDetector extends EventTarget {
  private observer?: MutationObserver;
  private originalFetch = window.fetch;
  private originalOpen = XMLHttpRequest.prototype.open;
  private originalSetHeader = XMLHttpRequest.prototype.setRequestHeader;
  private originalSend = XMLHttpRequest.prototype.send;
  private xhrState = new WeakMap<XMLHttpRequest, { url: string; headers: Headers }>();

  start(): void {
    const detector = this;
    window.fetch = async function (input, init) {
      detector.inspect(input, init);
      return detector.originalFetch.call(this, input, init);
    };

    XMLHttpRequest.prototype.open = function (
      method: string,
      url: string | URL,
      async = true,
      username?: string | null,
      password?: string | null
    ) {
      detector.xhrState.set(this, { url: String(url), headers: new Headers() });
      return detector.originalOpen.call(this, method, url, async as boolean, username, password);
    };
    XMLHttpRequest.prototype.setRequestHeader = function (name: string, value: string) {
      detector.xhrState.get(this)?.headers.set(name, value);
      return detector.originalSetHeader.call(this, name, value);
    };
    XMLHttpRequest.prototype.send = function (body?: Document | XMLHttpRequestBodyInit | null) {
      const state = detector.xhrState.get(this);
      if (state) detector.inspect(state.url, { headers: state.headers });
      return detector.originalSend.call(this, body);
    };

    this.observer = new MutationObserver(() => this.emitVideo());
    this.observer.observe(document.documentElement, { childList: true, subtree: true });
    this.emitVideo();
  }

  private inspect(input: RequestInfo | URL, init?: RequestInit): void {
    const url = new URL(input instanceof Request ? input.url : String(input), document.baseURI);
    const match = url.pathname.match(PLAYBACK_PATH);
    const session = sessionFromPlaybackRequest(input, init);
    if (match?.[1] && session) {
      this.dispatchEvent(new CustomEvent<PlaybackTarget>("playback", {
        detail: { itemId: decodeURIComponent(match[1]), session }
      }));
    }
  }

  private emitVideo(): void {
    const video = [...document.querySelectorAll("video")].at(-1);
    if (video) this.dispatchEvent(new CustomEvent<HTMLVideoElement>("video", { detail: video }));
  }
}
