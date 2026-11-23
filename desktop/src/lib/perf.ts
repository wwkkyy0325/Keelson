/// What the window spent its time on, measured from inside the window.
///
/// Two reports of a frozen conversation at full CPU arrived with no evidence
/// attached: the app could not say which path burned the core, so the only way
/// to find out was to guess from the outside. A profiler answers that for one
/// run; what is needed after the fact is an always-on counter of the few paths
/// that can run away — markdown samples, streamed deltas, poll ticks — plus the
/// stalls the browser itself noticed.
///
/// Every call here is a map lookup and an addition. The frame watcher exists
/// because WebKit does not report long tasks: a missed animation frame is the
/// portable evidence that the main thread was blocked. It is armed by activity
/// rather than left running, because a perpetual frame loop is itself a cost.

/// Samples kept per timer: enough to see a trend, bounded so a session that
/// runs for hours cannot grow the instrumentation.
export const SAMPLE_BOUND = 64;

/// A frame gap longer than this means the main thread was busy while an
/// animation frame was due; 50 ms is the long-task threshold browsers use.
export const STALL_MS = 50;

/// How long the frame watcher keeps asking for frames after the last counted
/// event. A `requestAnimationFrame` loop that never stops makes WebKit commit a
/// frame of an idle window forever: measured on a still transcript at 4–7.5% of
/// a core with `RenderLayer::paint` as the hot stack, and every layout the page
/// dirties is then painted at the frame rate instead of once. Frames only mean
/// something while work is happening, so the loop is armed by activity and
/// stops itself afterwards.
export const ARM_MS = 2000;

/// Whether the watcher is still wanted at `nowMs` given the instant it was last
/// armed for. Pure, so the arming rule is testable without a browser.
export function stillArmed(now: number, armUntil: number): boolean {
  return Number.isFinite(armUntil) && now < armUntil;
}

export type TimerStat = {
  count: number;
  totalMs: number;
  maxMs: number;
  lastMs: number;
  /// The most recent samples, oldest first.
  samples: number[];
};

export type StallStat = { count: number; totalMs: number; maxMs: number };

export type PerfReport = {
  uptimeMs: number;
  hidden: boolean;
  /// Named counts: streamed deltas per event, dispatched actions, poll ticks.
  counters: Record<string, number>;
  /// Named durations in milliseconds.
  timers: Record<string, TimerStat>;
  /// Frames the browser could not deliver on time.
  stalls: StallStat;
  /// Long tasks, when the engine reports them at all (`longtask` observer).
  longTasks: StallStat;
};

export function isStall(gapMs: number): boolean {
  return Number.isFinite(gapMs) && gapMs > STALL_MS;
}

function nowMs(): number {
  return typeof performance === "undefined" ? Date.now() : performance.now();
}

function add(stat: StallStat, ms: number): void {
  stat.count += 1;
  stat.totalMs += ms;
  stat.maxMs = Math.max(stat.maxMs, ms);
}

export class Perf {
  private began = nowMs();
  private hidden = false;
  private counters = new Map<string, number>();
  private timers = new Map<string, TimerStat>();
  private stalls: StallStat = { count: 0, totalMs: 0, maxMs: 0 };
  private longTasks: StallStat = { count: 0, totalMs: 0, maxMs: 0 };
  private installed = false;
  private lastFrame = 0;
  /// The instant the frame watcher may stop; see [`ARM_MS`].
  private armUntil = 0;
  /// A frame is already scheduled, so a burst of counts asks for one frame, not
  /// one frame each.
  private pending = false;

  /// Counts occurrences. Names are literals at every call site, so the map has
  /// as many entries as the source has counters and no cap is needed.
  count(name: string, by = 1): void {
    this.counters.set(name, (this.counters.get(name) ?? 0) + by);
    this.pulse();
  }

  /// Records one complete run of a measured path.
  mark(name: string, ms: number): void {
    let stat = this.timers.get(name);
    if (stat === undefined) {
      stat = { count: 0, totalMs: 0, maxMs: 0, lastMs: 0, samples: [] };
      this.timers.set(name, stat);
    }
    stat.count += 1;
    stat.totalMs += ms;
    stat.maxMs = Math.max(stat.maxMs, ms);
    stat.lastMs = ms;
    stat.samples.push(ms);
    if (stat.samples.length > SAMPLE_BOUND) stat.samples.shift();
    this.pulse();
  }

  /// Measures a synchronous run and records it under `name`.
  time<T>(name: string, run: () => T): T {
    const started = nowMs();
    try {
      return run();
    } finally {
      this.mark(name, nowMs() - started);
    }
  }

  /// Measures a promise. A poll that waits on the host is the one duration the
  /// window cannot take synchronously, and a poll that never returns looks of
  /// course like a fast one from the inside.
  async timeAsync<T>(name: string, run: () => Promise<T>): Promise<T> {
    const started = nowMs();
    try {
      return await run();
    } finally {
      this.mark(name, nowMs() - started);
    }
  }

  setHidden(hidden: boolean): void {
    this.hidden = hidden;
  }

  /// Installs the frame watcher, the long-task observer and the console handle.
  /// Tests call `report()` on a bare instance instead.
  install(): void {
    if (this.installed) return;
    this.installed = true;
    const doc = typeof document === "undefined" ? null : document;
    if (doc === null) return;
    this.hidden = doc.hidden === true;
    doc.addEventListener("visibilitychange", () => {
      this.setHidden(doc.hidden === true);
    });
    this.watchLongTasks();
    this.watchFrames();
    if (typeof window !== "undefined") {
      (window as unknown as { keelPerf?: unknown }).keelPerf = {
        report: () => this.report(),
        reset: () => this.reset(),
      };
    }
  }

  /// WebKit has no `longtask` entry type and throws on the observer; a missing
  /// measurement is fine because the frame watcher covers the same ground.
  private watchLongTasks(): void {
    if (typeof PerformanceObserver === "undefined") return;
    try {
      const observer = new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) add(this.longTasks, entry.duration);
      });
      observer.observe({ entryTypes: ["longtask"] });
    } catch {
      // Not supported here; frames still tell us about a blocked thread.
    }
  }

  private watchFrames(): void {
    if (typeof requestAnimationFrame === "undefined") return;
    const step = (time: number): void => {
      this.pending = false;
      if (this.lastFrame > 0) {
        const gap = time - this.lastFrame;
        if (isStall(gap)) add(this.stalls, gap);
      }
      this.lastFrame = time;
      if (stillArmed(nowMs(), this.armUntil)) {
        this.pending = true;
        requestAnimationFrame(step);
      } else {
        // Nothing has happened for a while: stop asking for frames, and forget
        // the last one so the first gap after a quiet spell is not a stall.
        this.lastFrame = 0;
        this.armUntil = 0;
      }
    };
    const arm = (): void => {
      if (this.pending) return;
      this.pending = true;
      requestAnimationFrame(step);
    };
    this.arm = arm;
  }

  /// Keeps the frame watcher running for [`ARM_MS`] from now. Every counted or
  /// timed path calls it; the cost is an addition and, at most, one scheduled
  /// frame.
  private pulse(): void {
    const until = nowMs() + ARM_MS;
    if (until > this.armUntil) this.armUntil = until;
    this.arm?.();
  }

  private arm: (() => void) | null = null;

  report(): PerfReport {
    const counters: Record<string, number> = {};
    for (const [name, value] of this.counters) counters[name] = value;
    const timers: Record<string, TimerStat> = {};
    for (const [name, stat] of this.timers) {
      timers[name] = { ...stat, samples: [...stat.samples] };
    }
    return {
      uptimeMs: Math.round(nowMs() - this.began),
      hidden: this.hidden,
      counters,
      timers,
      stalls: { ...this.stalls },
      longTasks: { ...this.longTasks },
    };
  }

  reset(): void {
    this.began = nowMs();
    this.counters.clear();
    this.timers.clear();
    this.stalls = { count: 0, totalMs: 0, maxMs: 0 };
    this.longTasks = { count: 0, totalMs: 0, maxMs: 0 };
    this.lastFrame = 0;
  }
}

/// The app's instrumentation. `window.keelPerf.report()` in the webview console
/// prints what this collected.
export const perf = new Perf();
