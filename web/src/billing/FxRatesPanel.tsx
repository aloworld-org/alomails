// The exchange rates a tenant's foreign-currency documents are converted at
// (alo Billing, ADR 0035, wave B1.21).
//
// It lives inside the billing settings page rather than on a page of its own,
// under the accounting currency that gives it its meaning: a tenant that
// invoices only in its own currency never needs a rate, and a rail entry for
// something most tenants never open is a rail entry in the way.
//
// The module's rules hold here too. **No arithmetic**: a rate is typed as the
// decimal it was published as and sent as that string, and every rate shown is
// the server's own formatting of its stored integer — the browser never divides
// micro-units into a rate, and never converts an amount. **No validation**: what
// a rate may be, which currencies may be quoted, and what a rate file must look
// like are the store's rules, and a refusal is shown in the server's own words
// with the form intact. And **nothing is fetched from anywhere**: the file is one
// the user pastes, which is what makes their books' conversion auditable.
import { useCallback, useEffect, useState } from "react";

import { Button, Input, Spinner, Table, Td, Th } from "../ds";
import { strings, useLocale } from "../i18n";
import { billingMessage, useBillingApi } from "./api";
import { formatDocumentDate } from "./dates";
import { ErrorBanner, Field } from "./parts";
import type { FxRate } from "./types";
import styles from "./billingStyles";

/** The blank "add a rate" form. */
const BLANK = { currency: "", date: "", rate: "" };

/** How a stored rate got there, in words rather than in the wire's code. */
function sourceLabel(source: FxRate["source"]): string {
  return source === "ecb"
    ? strings.billingFxSourceEcb
    : strings.billingFxSourceManual;
}

export function FxRatesPanel() {
  const api = useBillingApi();
  const locale = useLocale();
  const [rates, setRates] = useState<FxRate[] | null>(null);
  const [form, setForm] = useState(BLANK);
  const [file, setFile] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      setRates(await api.fxRates());
      setError(null);
    } catch (err) {
      setError(billingMessage(err, strings.billingFxLoadFailed));
    }
  }, [api]);

  useEffect(() => {
    void load();
  }, [load]);

  /** Adds or corrects one rate. The three fields go as typed: the server
   *  canonicalises the currency and reads the decimal itself. */
  async function add() {
    setBusy(true);
    try {
      const saved = await api.saveFxRate(form);
      setForm(BLANK);
      setNote(strings.billingFxAddSaved(saved.currency, saved.date));
      setError(null);
      await load();
    } catch (err) {
      setError(billingMessage(err, strings.billingSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  /** Imports a pasted reference-rate file. All or nothing on the server, so
   *  either the note below counts what landed or the banner says what is wrong
   *  with which row. */
  async function importFile() {
    setBusy(true);
    try {
      const summary = await api.importFxRates(file);
      setFile("");
      setNote(strings.billingFxImported(summary.rates, summary.days));
      setError(null);
      await load();
    } catch (err) {
      setError(billingMessage(err, strings.billingSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  const readable = (day: string) => formatDocumentDate(day, locale, day);

  return (
    <div className={styles.lines}>
      <p className={styles.hint}>{strings.billingFxIntro}</p>
      {error !== null && <ErrorBanner message={error} />}
      {note !== null && (
        <p className={styles.hint} role="status">
          {note}
        </p>
      )}

      <div className={styles.row}>
        <Field label={strings.billingColCurrency}>
          <Input
            value={form.currency}
            maxLength={3}
            autoCapitalize="characters"
            autoCorrect="off"
            spellCheck={false}
            onChange={(e) => setForm({ ...form, currency: e.target.value })}
          />
        </Field>
        <Field label={strings.billingFxColDate}>
          <Input
            type="date"
            value={form.date}
            onChange={(e) => setForm({ ...form, date: e.target.value })}
          />
        </Field>
        <Field
          label={strings.billingFxColRate}
          hint={strings.billingFxRateHint}
        >
          <Input
            value={form.rate}
            inputMode="decimal"
            onChange={(e) => setForm({ ...form, rate: e.target.value })}
          />
        </Field>
        <Button
          variant="ghost"
          onClick={() => void add()}
          disabled={
            busy ||
            form.currency.trim() === "" ||
            form.date === "" ||
            form.rate.trim() === ""
          }
        >
          {strings.billingFxAdd}
        </Button>
      </div>

      <Field label={strings.billingFxImport} hint={strings.billingFxImportHint}>
        <textarea
          className={styles.textarea}
          value={file}
          rows={3}
          spellCheck={false}
          onChange={(e) => setFile(e.target.value)}
        />
      </Field>
      <div className={styles.createBar}>
        {busy && <Spinner size={16} />}
        <Button
          variant="ghost"
          onClick={() => void importFile()}
          disabled={busy || file.trim() === ""}
        >
          {strings.billingFxImportRun}
        </Button>
      </div>

      {rates !== null && rates.length === 0 ? (
        <p className={styles.hint}>{strings.billingFxEmpty}</p>
      ) : (
        <Table label={strings.billingFxRates} density="compact">
          <thead>
            <tr>
              <Th>{strings.billingColCurrency}</Th>
              <Th>{strings.billingFxColDate}</Th>
              <Th numeric>{strings.billingFxColRate}</Th>
              <Th>{strings.billingFxColSource}</Th>
            </tr>
          </thead>
          <tbody>
            {(rates ?? []).map((rate) => (
              <tr key={`${rate.currency}-${rate.date}`}>
                <td>{rate.currency}</td>
                <td>{readable(rate.date)}</td>
                {/* The server's own formatting of its stored integer: the
                    browser never turns micro-units back into a rate. */}
                <Td numeric className={styles.mono}>
                  {rate.rate}
                </Td>
                <td>{sourceLabel(rate.source)}</td>
              </tr>
            ))}
          </tbody>
        </Table>
      )}
    </div>
  );
}
