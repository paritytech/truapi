// Discovered UI targets for the headless full-bundle spike.
// Values verified against playground/src on this branch — see
// .superpowers/sdd/task-1-report.md for file:line evidence. Update here if
// the playground UI moves; this is the single place UI knowledge lives —
// no other task may hardcode a selector.

// The Diagnosis screen is deep-linked from the root page via a `view` query
// param (see playground/src/app/page.tsx: VIEW_PARAM / SERVICE_FOR_VIEW /
// selectionFromUrl). Loading this path directly renders DiagnosisView with
// no need to click through the left-rail "Diagnosis" entry first.
export const DIAGNOSIS_PATH = "/?view=diagnosis";

// The Diagnosis run does NOT auto-start on mount: `runDiagnosis` is only
// invoked from DiagnosisView's onRun callback, which is wired to this
// button's onClick. There is no effect anywhere that calls it automatically.
export const RUN_ALL_SELECTOR: string | null = '[data-testid="diagnosis-run"]';

// Text content is "{passCount} success · {failCount} failed"; the existing
// e2e test parses the failed count from this same element with
// /(\d+)\s+failed\b/.
export const FAILED_COUNT_SELECTOR = '[data-testid="diagnosis-summary"]';

// Report body; carries data-report-ready="true" once the run has finished
// rendering its markdown (DiagnosisView.tsx:173-174 — the `<pre>` sibling of
// diagnosis-copy-report, rendered only when `hasResults && !isRunning`;
// data-report-ready is `reportMarkdown.length > 0`, which React stringifies
// to the literal attribute value "true").
export const REPORT_READY_SELECTOR =
  '[data-testid="diagnosis-report-markdown"][data-report-ready="true"]';
