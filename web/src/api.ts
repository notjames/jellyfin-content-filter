import type { FilterConfig, TitleControls, ViewerDefaults } from "./types";

export interface Session {
  baseUrl: string;
  headers: Headers;
}

function normalizeBase(url: URL): string {
  const marker = /\/Items\/[^/]+\/PlaybackInfo\/?$/i;
  return url.pathname.replace(marker, "").replace(/\/$/, "");
}

export class FilterApi {
  constructor(private readonly session: Session) {}

  private request<T>(path: string, init?: RequestInit): Promise<T> {
    const url = `${this.session.baseUrl}/ContentFilter${path}`;
    const headers = new Headers(this.session.headers);
    headers.set("Accept", "application/json");
    if (init?.body) headers.set("Content-Type", "application/json");
    return fetch(url, { ...init, headers, credentials: "same-origin" }).then(async (response) => {
      if (!response.ok) throw new Error(`Content Filter API returned ${response.status}`);
      return response.json() as Promise<T>;
    });
  }

  getConfig(itemId: string): Promise<FilterConfig> {
    return this.request(`/Items/${encodeURIComponent(itemId)}`);
  }

  saveViewer(defaults: ViewerDefaults): Promise<void> {
    return this.request<void>("/Viewer", { method: "PUT", body: JSON.stringify(defaults) });
  }

  saveTitle(itemId: string, controls: TitleControls): Promise<void> {
    return this.request<void>(`/Items/${encodeURIComponent(itemId)}`, {
      method: "PUT",
      body: JSON.stringify(controls)
    });
  }
}

export function sessionFromPlaybackRequest(input: RequestInfo | URL, init?: RequestInit): Session | null {
  const request = input instanceof Request ? input : null;
  const url = new URL(request?.url ?? String(input), document.baseURI);
  if (!/\/Items\/[^/]+\/PlaybackInfo\/?$/i.test(url.pathname)) return null;
  const headers = new Headers(request?.headers);
  new Headers(init?.headers).forEach((value, key) => headers.set(key, value));
  return { baseUrl: `${url.origin}${normalizeBase(url)}`, headers };
}

export function sessionFromGlobalApiClient(): Session | null {
  const client = (window as typeof window & {
    ApiClient?: {
      getUrl(path: string): string;
      accessToken?: () => string | null | undefined;
    };
  }).ApiClient;
  if (!client?.getUrl) return null;

  const endpoint = new URL(client.getUrl("/ContentFilter"), document.baseURI);
  const headers = new Headers();
  const token = client.accessToken?.();
  if (token) headers.set("X-Emby-Token", token);
  endpoint.pathname = endpoint.pathname.replace(/\/ContentFilter\/?$/i, "").replace(/\/$/, "");
  endpoint.search = "";
  endpoint.hash = "";
  return { baseUrl: `${endpoint.origin}${endpoint.pathname}`, headers };
}
