import type { FilterApi } from "./api";
import type { Action, FilterConfig } from "./types";

const ACTIONS: Action[] = ["allow", "skip", "mute"];

export class FilterUi {
  private readonly host = document.createElement("div");
  private readonly root = this.host.attachShadow({ mode: "open" });
  private config?: FilterConfig;
  private itemId?: string;
  private api?: FilterApi;

  constructor(private readonly onConfigChanged?: (config: FilterConfig) => void) {
    this.host.id = "jcf-root";
    this.root.innerHTML = `
      <style>
        :host{position:fixed;right:16px;bottom:80px;z-index:999999;font:14px system-ui;color:#fff}
        button{border:0;border-radius:5px;padding:9px 12px;background:#00a4dc;color:#fff;cursor:pointer}
        #warning{display:none;max-width:360px;margin-bottom:8px;padding:10px;border-radius:5px;background:#8b1e1e;box-shadow:0 2px 8px #0008}
        dialog{width:min(520px,calc(100vw - 32px));color:#eee;background:#202020;border:1px solid #555;border-radius:8px}
        label{display:grid;grid-template-columns:1fr 120px 120px;gap:8px;align-items:center;margin:8px 0}
        select{padding:6px;background:#333;color:#fff;border:1px solid #666}.actions{display:flex;justify-content:flex-end;gap:8px;margin-top:16px}
      </style>
      <div id="warning" role="alert"></div>
      <button id="open" aria-label="Content filter settings">Content filter</button>
      <dialog><h2>Content filter</h2><p>Choose viewer defaults and optional overrides for this title.</p>
        <div id="categories"></div><div class="actions"><button id="cancel">Cancel</button><button id="save">Save</button></div>
      </dialog>`;
    this.root.querySelector("#open")?.addEventListener("click", () => this.open());
    this.root.querySelector("#cancel")?.addEventListener("click", () => this.dialog.close());
    this.root.querySelector("#save")?.addEventListener("click", () => void this.save());
    document.body.append(this.host);
  }

  bind(itemId: string, api: FilterApi, config: FilterConfig): void {
    this.itemId = itemId;
    this.api = api;
    this.config = config;
    this.warning("");
  }

  warning(message: string): void {
    const element = this.root.querySelector<HTMLElement>("#warning");
    if (!element) return;
    element.textContent = message;
    element.style.display = message ? "block" : "none";
  }

  private get dialog(): HTMLDialogElement {
    return this.root.querySelector("dialog") as HTMLDialogElement;
  }

  private open(): void {
    const container = this.root.querySelector("#categories");
    if (!container || !this.config) {
      this.warning("Content filtering is unavailable. Playback is continuing unfiltered.");
      return;
    }
    const categories = new Set([
      ...Object.keys(this.config.viewer.categories),
      ...Object.keys(this.config.title?.categories ?? {}),
      ...this.config.intervals.map((entry) => entry.category)
    ]);
    container.replaceChildren(...[...categories].sort().map((category) => this.row(category)));
    this.dialog.showModal();
  }

  private row(category: string): HTMLElement {
    const label = document.createElement("label");
    label.dataset.category = category;
    const name = document.createElement("span");
    name.textContent = category;
    label.append(name, this.select("viewer", this.config?.viewer.categories[category] ?? "allow", false));
    label.append(this.select("title", this.config?.title?.categories?.[category] ?? "", true));
    return label;
  }

  private select(kind: string, value: Action | "", optional: boolean): HTMLSelectElement {
    const select = document.createElement("select");
    select.dataset.kind = kind;
    const values = optional ? ["", ...ACTIONS] : ACTIONS;
    for (const action of values) {
      const option = new Option(action || "Use viewer default", action);
      option.selected = action === value;
      select.add(option);
    }
    return select;
  }

  private async save(): Promise<void> {
    if (!this.api || !this.itemId || !this.config) return;
    const viewer: Record<string, Action> = {};
    const title: Record<string, Action> = {};
    for (const row of this.root.querySelectorAll<HTMLElement>("[data-category]")) {
      const category = row.dataset.category;
      const selects = row.querySelectorAll<HTMLSelectElement>("select");
      if (!category || selects.length < 2) continue;
      viewer[category] = selects[0]?.value as Action;
      if (selects[1]?.value) title[category] = selects[1].value as Action;
    }
    try {
      await Promise.all([
        this.api.saveViewer({ categories: viewer }),
        this.api.saveTitle(this.itemId, { categories: title })
      ]);
      this.config = {
        ...this.config,
        viewer: { categories: viewer },
        title: { categories: title }
      };
      this.onConfigChanged?.(this.config);
      this.dialog.close();
      this.warning("");
    } catch (error) {
      this.warning(`${String(error)}. Playback is continuing unfiltered.`);
    }
  }
}
