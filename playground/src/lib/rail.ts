// Single source for the index (left rail) row/section `data-testid`s and the
// scroll-into-view behavior shared by the breadcrumb, method title, and the
// back-to-index scroll restore.

export const serviceTestId = (service: string): string => `service-${service}`;
export const methodTestId = (service: string, method: string): string =>
  `method-${service}-${method}`;

/** Scroll an index element into view (and optionally focus it). */
export function revealInRail(
  testId: string,
  opts: { block: ScrollLogicalPosition; smooth?: boolean; focus?: boolean },
): void {
  const el = document.querySelector(`[data-testid="${testId}"]`);
  if (!(el instanceof HTMLElement)) return;
  el.scrollIntoView({ block: opts.block, behavior: opts.smooth ? "smooth" : "auto" });
  if (opts.focus) el.focus({ preventScroll: true });
}
