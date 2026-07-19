import { sessionFromPlaybackRequest, type Session } from "../../web/src/api";

export interface DesktopPlaybackTarget {
  itemId: string;
  session: Session;
}

const PLAYBACK_PATH = /\/Items\/([^/]+)\/PlaybackInfo\/?$/i;

export class DesktopPlaybackDetector extends EventTarget {
  private readonly originalFetch = window.fetch;
  private readonly originalOpen = XMLHttpRequest.prototype.open;
  private readonly originalSetHeader = XMLHttpRequest.prototype.setRequestHeader;
  private readonly originalSend = XMLHttpRequest.prototype.send;
  private readonly xhrState = new WeakMap<XMLHttpRequest, { url: string; headers: Headers }>();

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
  }

  private inspect(input: RequestInfo | URL, init?: RequestInit): void {
    const url = new URL(input instanceof Request ? input.url : String(input), document.baseURI);
    const match = url.pathname.match(PLAYBACK_PATH);
    const session = sessionFromPlaybackRequest(input, init);
    if (match?.[1] && session) {
      this.dispatchEvent(new CustomEvent<DesktopPlaybackTarget>("playback", {
        detail: { itemId: decodeURIComponent(match[1]), session }
      }));
    }
  }
}
