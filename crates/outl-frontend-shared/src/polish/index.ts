/**
 * Polish shared by both GUI clients: motion (fold, drag, sync settle),
 * the long-reading rhythm, focus mode and the first-run tips.
 * Styles live in `@outl/shared/polish/styles`.
 */
export { createFocusMode, focusMode, FOCUS_KEY, type FocusMode } from "./focus";
export { isBlankOutline, firstRunTips, tipsDismissed, dismissTips, TIPS_KEY, type Tip, type TipClient } from "./firstRun";
export { FirstRunTips } from "./FirstRunTips";
export { contrastRatio } from "./contrast";
export { createSettle } from "./settle";
