// UI targets for the headless full-bundle driver — the single place selector
// knowledge lives; update here when the playground UI moves.

// Deep-links straight to DiagnosisView (playground/src/app/page.tsx), skipping
// the left-rail click-through.
export const DIAGNOSIS_PATH = "/?view=diagnosis";

// Diagnosis never auto-starts on mount (DiagnosisView) — this button's onClick
// is the only trigger.
export const RUN_ALL_SELECTOR = '[data-testid="diagnosis-run"]';

// Connection status chip rendered by StatusChip (playground/src/app/page.tsx).
export const STATUS_CHIP_SELECTOR = ".status__label";

// Summary text reads "{passCount} success · {failCount} failed" (DiagnosisView).
export const FAILED_COUNT_SELECTOR = '[data-testid="diagnosis-summary"]';

// Parses the "N success · M failed" summary text.
export const FAILED_COUNT_PATTERN = /(\d+)\s+failed\b/;

// Set once the run has finished rendering its report (DiagnosisView).
export const REPORT_READY_SELECTOR =
  '[data-testid="diagnosis-report-markdown"][data-report-ready="true"]';

// A single Diagnosis result row in the failed state (DiagnosisView).
export const FAILED_ROW_SELECTOR = '[data-testid="diagnosis-row"][data-status="fail"]';
