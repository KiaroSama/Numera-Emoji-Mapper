// The constants panel.html's inline block defines before the scripts load.
// Everything else the scripts share is a top-level name in one of them, which
// the checker already sees because every file is in the same program.
declare const TOKEN: string;
declare const PREVIEW_TIERS: {
  full: { size: number; fps: number };
  compact: { size: number; fps: number };
};
declare const HIDDEN: number;
declare const STALE: string;
declare const PER_SET: number;

// Every listener in these scripts is attached to an element, so an event's
// target is always one: `closest` is what they call on it.
interface EventTarget { closest(selectors: string): HTMLElement | null; }
// querySelector results are read for their data-* attributes.
interface Element { readonly dataset: DOMStringMap; }
