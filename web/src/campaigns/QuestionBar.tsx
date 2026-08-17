// The question, as a colleague writes it (ADR 0044, C1.5).
//
// A segment is **conditions, never people**, so this is the whole of it: who,
// where, and whether they have bought. It writes nothing — the count beside it
// moves as the fields change, and saving is a separate, deliberate act.
//
// Two things here are the interface laws rather than taste. The period select
// is disabled while the purchase condition is "anyone", because a period with
// no condition is refused by the server and offering it would be an invitation
// to an error. And the countries box says what it wants in its hint rather than
// in a manual: `BE, NL`, and empty means everywhere — not nowhere.
import { Field, Input, Select } from "../ds";
import { strings } from "../i18n";
import { splitCountries } from "./format";
import type { PurchaseConditionKind, SegmentConditions } from "./types";
import styles from "./CampaignsModule.module.css";

/** The periods offered. "Ever" is a real answer, not a missing one: *has never
 *  bought from us* is a question a shop asks. */
const PERIODS = [30, 90, 180, 365] as const;

export function QuestionBar({
  countries,
  conditions,
  onCountries,
  onConditions,
}: {
  /** The countries box exactly as typed, so a half-typed code is not thrown
   *  away by a re-render. The conditions carry the parsed version. */
  countries: string;
  conditions: SegmentConditions;
  onCountries: (raw: string) => void;
  onConditions: (conditions: SegmentConditions) => void;
}) {
  const purchase = conditions.purchase;

  function setCountries(raw: string) {
    onCountries(raw);
    onConditions({ ...conditions, countries: splitCountries(raw) });
  }

  function setCondition(value: string) {
    if (value === "") {
      onConditions({ ...conditions, purchase: null });
      return;
    }
    onConditions({
      ...conditions,
      purchase: {
        condition: value as PurchaseConditionKind,
        withinDays: purchase?.withinDays ?? null,
      },
    });
  }

  function setPeriod(value: string) {
    if (purchase === null) return;
    onConditions({
      ...conditions,
      purchase: { ...purchase, withinDays: value === "" ? null : Number(value) },
    });
  }

  return (
    <div className={styles.question}>
      <Field label={strings.campaignsCountriesLabel} hint={strings.campaignsCountriesHint}>
        {(control) => (
          <Input
            {...control}
            value={countries}
            placeholder={strings.campaignsCountriesPlaceholder}
            onChange={(e) => setCountries(e.target.value)}
          />
        )}
      </Field>

      <Field label={strings.campaignsPurchaseLabel}>
        {(control) => (
          <Select
            {...control}
            value={purchase?.condition ?? ""}
            onChange={(e) => setCondition(e.target.value)}
          >
            <option value="">{strings.campaignsPurchaseAny}</option>
            <option value="bought">{strings.campaignsPurchaseBought}</option>
            <option value="not_bought">{strings.campaignsPurchaseNotBought}</option>
          </Select>
        )}
      </Field>

      <Field label={strings.campaignsPeriodLabel}>
        {(control) => (
          <Select
            {...control}
            value={purchase?.withinDays === null || purchase === null ? "" : String(purchase.withinDays)}
            disabled={purchase === null}
            onChange={(e) => setPeriod(e.target.value)}
          >
            <option value="">{strings.campaignsPeriodEver}</option>
            {PERIODS.map((days) => (
              <option key={days} value={days}>
                {strings.campaignsPeriodDays(days)}
              </option>
            ))}
          </Select>
        )}
      </Field>
    </div>
  );
}
