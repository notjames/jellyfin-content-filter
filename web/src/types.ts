export type Action = "skip" | "mute" | "allow";

export type FilterStatus = "ready" | "partial" | "failed" | "missing";

export interface Interval {
  start: number;
  end: number;
  category: string;
  action?: Action;
}

export interface ViewerDefaults {
  categories: Record<string, Action>;
}

export interface TitleControls {
  categories?: Record<string, Action>;
}

export interface FilterProblem {
  code: string;
  message: string;
}

export interface FilterConfig {
  viewer: ViewerDefaults;
  title?: TitleControls;
  intervals: Interval[];
  filter_status?: FilterStatus;
  problems?: FilterProblem[];
  filters_enabled?: boolean;
  warning?: string | null;
}

export interface EffectiveInterval extends Interval {
  action: Action;
}

export function resolveIntervals(config: FilterConfig): EffectiveInterval[] {
  if (config.filters_enabled === false) return [];
  return config.intervals
    .map((interval) => ({
      ...interval,
      action:
        interval.action ??
        config.title?.categories?.[interval.category] ??
        config.viewer.categories[interval.category] ??
        "allow"
    }))
    .filter((interval) => interval.end > interval.start)
    .sort((a, b) => a.start - b.start || a.end - b.end);
}

export function maskSubtitleText(text: string): string {
  return text.replace(/\S/g, "*");
}
