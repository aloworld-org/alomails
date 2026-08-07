// How a recurring arrangement's rhythm is worded (B2.11) — one place, so the
// picker in the dialog and the column in the list can never call the same
// cadence two different things.
import { strings } from "../i18n";
import type { ScheduleCadence } from "./types";

/** The four rhythms, in the order a business thinks of them: shortest first. */
export const CADENCES = [
  { value: "weekly", label: () => strings.billingCadenceWeekly },
  { value: "monthly", label: () => strings.billingCadenceMonthly },
  { value: "quarterly", label: () => strings.billingCadenceQuarterly },
  { value: "yearly", label: () => strings.billingCadenceYearly },
] as const satisfies readonly { value: ScheduleCadence; label: () => string }[];

/** What to call a cadence. One the server knows and this client does not is
 *  shown verbatim rather than blanked — the same rule the status chips follow. */
export function cadenceLabel(cadence: ScheduleCadence): string {
  return CADENCES.find((c) => c.value === cadence)?.label() ?? cadence;
}
