export type Action = "skip" | "mute" | "allow";

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

export interface FilterConfig {
  viewer: ViewerDefaults;
  title?: TitleControls;
  intervals: Interval[];
}

export interface EffectiveInterval extends Interval {
  action: Action;
}

export function resolveIntervals(config: FilterConfig): EffectiveInterval[] {
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
