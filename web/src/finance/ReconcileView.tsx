// Reconciliation: the pile of bank lines nobody has attributed yet, what the
// server thinks each one is, and the one confirm that settles it.
//
// The design note's shape, kept: unmatched lines on the left, the suggestion
// and its evidence on the right, one confirm per line and an undo beside it.
//
// **A suggestion is a suggestion.** The two guessing stages are worth exactly
// as much as the person looking at them (ADR 0023): nothing on this screen
// happens without a click, the evidence is spelled out beside every guess in
// the reader's own language, and the manual pick sits beside them for the line
// they got wrong or had nothing to say about.
//
// **Every act is undoable and the undo is beside the act.** Taking a match back
// reverses the entry with an entry of its own — this screen never hides that
// behind a confirmation dialog, because the way back is real and a dialog would
// suggest it is not.
//
// **This screen computes no money.** What a line moves, what a document still
// owes and what may be attributed to it are the server's, and the amount sent
// with a confirm is the line's own `amountCents` — compared under the row locks
// rather than believed, so a screen a colleague changed under us is a refusal.
import { useCallback, useEffect, useMemo, useState } from "react";
import { CheckCheck, Search, Sparkles } from "lucide-react";

import type { BillingInvoiceSummary } from "../billing";
import {
  Badge,
  Button,
  Checkbox,
  Input,
  Select,
  Spinner,
  Table,
  Td,
  Th,
  Toolbar,
  ToolbarSpacer,
  cx,
  useDialogs,
} from "../ds";
import { strings } from "../i18n";
import { financeMessage, useFinanceApi } from "./api";
import {
  amountLabel,
  dayLabel,
  evidenceLabel,
  lineStatusLabel,
  lineStatusTone,
} from "./format";
import { BADGE_TONE, EmptyState, ErrorBanner } from "./parts";
import { InvoicePicker } from "./InvoicePicker";
import type {
  BankLine,
  BankStatement,
  BankSuggestions,
  LikelyMatch,
} from "./types";
import styles from "./FinanceModule.module.css";

/** Everything the screen is drawn from, in one read each. */
interface Pile {
  suggestions: BankSuggestions;
  settled: BankLine[];
  setAside: BankLine[];
}

const NOTHING: Pile = {
  suggestions: { lines: [], numbersCapped: false, ledgerCapped: false },
  settled: [],
  setAside: [],
};

export function ReconcileView({
  revision: outerRevision,
}: {
  revision: number;
}) {
  const api = useFinanceApi();
  const dialogs = useDialogs();
  const [statements, setStatements] = useState<BankStatement[]>([]);
  const [statement, setStatement] = useState("");
  const [query, setQuery] = useState("");
  const [confidence, setConfidence] = useState("all");
  const [selected, setSelected] = useState<string[]>([]);
  const [pile, setPile] = useState<Pile>(NOTHING);
  const [picking, setPicking] = useState<BankLine | null>(null);
  const [pickError, setPickError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);

  const reload = useCallback(() => setRevision((r) => r + 1), []);

  // The imports, for the narrowing select. Loaded once: a statement list does
  // not change while somebody works a month, and reloading it on every confirm
  // would be a read per click for a control nobody touched.
  useEffect(() => {
    let live = true;
    void api
      .bankStatements()
      .then((list) => {
        if (live) setStatements(list);
      })
      .catch(() => {
        // The narrowing is a convenience; the pile below is the screen. A
        // failure here is reported by that read, not twice.
      });
    return () => {
      live = false;
    };
  }, [api, outerRevision]);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void (async () => {
      const narrow = statement === "" ? undefined : statement;
      try {
        // Three reads, one wait. The three lists are three states of one pile
        // and a screen that loaded them in sequence would show a line in none
        // of them for an instant after every confirm.
        const [suggestions, settled, setAside] = await Promise.all([
          api.bankSuggestions(narrow),
          api.bankLines({
            ...(narrow === undefined ? {} : { statement: narrow }),
            status: "matched",
          }),
          api.bankLines({
            ...(narrow === undefined ? {} : { statement: narrow }),
            status: "ignored",
          }),
        ]);
        if (live) {
          setPile({ suggestions, settled, setAside });
          setSelected([]);
          setError(null);
        }
      } catch (err) {
        if (live) setError(financeMessage(err, strings.financeBankLoadFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, statement, revision, outerRevision]);

  /** Attributes a line to a document. `amountCents` is the line's own: one line
   *  settles one document, and the server compares rather than believes. */
  async function match(
    line: BankLine,
    invoiceId: string,
    ruleId?: string | null,
  ) {
    setBusy(line.id);
    setError(null);
    setPickError(null);
    try {
      await api.matchBankLine(
        line.id,
        invoiceId,
        line.amountCents,
        ruleId ?? null,
      );
      setPicking(null);
      reload();
      return true;
    } catch (err) {
      const message = financeMessage(err, strings.financeBankMatchFailed);
      // A refusal on a hand-picked document belongs in the dialog the pick was
      // made in — the next pick is the correction, and closing the dialog to
      // show the sentence elsewhere would throw away the list they were reading.
      if (picking === null) setError(message);
      else setPickError(message);
      return false;
    } finally {
      setBusy(null);
    }
  }

  /** Takes a match back. No confirmation: the reversal is a real entry and the
   *  undo is the ordinary correction this screen exists to make. */
  async function unmatch(line: BankLine) {
    setBusy(line.id);
    setError(null);
    try {
      await api.unmatchBankLine(line.id);
      reload();
    } catch (err) {
      setError(financeMessage(err, strings.financeBankUnmatchFailed));
    } finally {
      setBusy(null);
    }
  }

  /** Sets a line aside, with the reason the server requires — a line nobody can
   *  judge later is a line somebody re-opens at the year end. */
  async function ignore(line: BankLine) {
    const reason = await dialogs.prompt({
      title: strings.financeBankIgnoreTitle,
      message: strings.financeBankIgnoreBody,
      confirmLabel: strings.financeBankIgnore,
      placeholder: strings.financeBankIgnorePlaceholder,
    });
    if (reason === null) return;
    setBusy(line.id);
    setError(null);
    try {
      await api.ignoreBankLine(line.id, reason);
      reload();
    } catch (err) {
      setError(financeMessage(err, strings.financeBankIgnoreFailed));
    } finally {
      setBusy(null);
    }
  }

  /** Back in the pile, with the reason cleared. */
  async function unignore(line: BankLine) {
    setBusy(line.id);
    setError(null);
    try {
      await api.unignoreBankLine(line.id);
      reload();
    } catch (err) {
      setError(financeMessage(err, strings.financeBankIgnoreFailed));
    } finally {
      setBusy(null);
    }
  }

  const { suggestions, settled, setAside } = pile;
  const visible = useMemo(() => suggestions.lines.filter((entry) => {
    const needle = query.trim().toLocaleLowerCase();
    const haystack = [entry.line.counterpartyName, entry.line.counterpartyIban, entry.line.remittance, entry.line.bankRef].filter(Boolean).join(" ").toLocaleLowerCase();
    const bestScore = entry.exact.length > 0 ? 100 : (entry.likely[0]?.score ?? 0);
    const matchesConfidence = confidence === "all" || (confidence === "certain" && bestScore >= 90) || (confidence === "review" && bestScore > 0 && bestScore < 90) || (confidence === "none" && bestScore === 0);
    return matchesConfidence && (needle === "" || haystack.includes(needle));
  }), [confidence, query, suggestions.lines]);

  const selectable = useMemo(() => visible.filter((entry) => entry.exact[0] !== undefined || entry.likely[0] !== undefined), [visible]);

  async function confirmSelected() {
    setBusy("bulk");
    setError(null);
    try {
      for (const entry of selectable.filter((candidate) => selected.includes(candidate.line.id))) {
        const exact = entry.exact[0];
        const likely = entry.likely[0];
        const candidate = exact ?? likely;
        if (candidate === undefined) continue;
        await api.matchBankLine(entry.line.id, candidate.invoiceId, entry.line.amountCents, exact === undefined ? likely?.ruleId ?? null : null);
      }
      reload();
    } catch (err) {
      setError(financeMessage(err, strings.financeBankMatchFailed));
      reload();
    } finally {
      setBusy(null);
    }
  }

  if (
    loading &&
    suggestions.lines.length === 0 &&
    settled.length === 0 &&
    setAside.length === 0
  ) {
    return (
      <div className={styles.page}>
        <Spinner size={20} />
      </div>
    );
  }

  return (
    <div className={styles.page}>
      <Toolbar label={strings.financeBankFilters}>
        <label className={styles.periodField}>
          {strings.financeBankStatement}
          {/* "Every statement" is an answer somebody must be able to return
              to, so it stays choosable. */}
          <Select
            value={statement}
            placeholder={strings.financeBankAllStatements}
            onChange={(e) => setStatement(e.target.value)}
          >
            {statements.map((one) => (
              <option key={one.id} value={one.id}>
                {`${one.accountIban} · ${dayLabel(one.fromDate, "—")} – ${dayLabel(one.toDate, "—")}`}
              </option>
            ))}
          </Select>
        </label>
        <label className="relative min-w-[15rem] flex-1 max-w-sm">
          <span className="sr-only">{strings.financeBankSearch}</span>
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-tertiary" aria-hidden="true" />
          <Input className="w-full pl-9" value={query} placeholder={strings.financeBankSearchPlaceholder} onChange={(event) => setQuery(event.target.value)} />
        </label>
        <Select value={confidence} aria-label={strings.financeBankConfidence} onChange={(event) => setConfidence(event.target.value)}>
          <option value="all">{strings.financeBankAllConfidence}</option>
          <option value="certain">{strings.financeBankCertain}</option>
          <option value="review">{strings.financeBankReviewSuggested}</option>
          <option value="none">{strings.financeBankNoSuggestion}</option>
        </Select>
        <ToolbarSpacer />
        {selectable.length > 0 && <Checkbox checked={selected.length === selectable.length} label={strings.financeBankSelectSuggested} onChange={(checked) => setSelected(checked ? selectable.map((entry) => entry.line.id) : [])} disabled={busy !== null} />}
        {selected.length > 0 && <Button size="sm" disabled={busy !== null} onClick={() => void confirmSelected()}><Sparkles className="size-4" />{strings.financeBankConfirmSelected(selected.length)}</Button>}
        {loading && <Spinner size={16} />}
      </Toolbar>

      {error !== null && <ErrorBanner message={error} />}
      {/* Never silent about a short list: a bookkeeper who sees five lines and
          concludes there is nothing left to match has been misled by us. */}
      {(suggestions.numbersCapped || suggestions.ledgerCapped) && (
        <p className={styles.notice}>{strings.financeBankCapped}</p>
      )}

      <section className={styles.section}>
        <h2 className={styles.sectionTitle}>
          {strings.financeBankToMatchTitle(suggestions.lines.length)}
        </h2>
        {suggestions.lines.length === 0 ? (
          <EmptyState
            Icon={CheckCheck}
            title={strings.financeBankAllMatchedTitle}
            body={strings.financeBankAllMatchedBody}
          />
        ) : (
          <ul className={styles.lineList}>
            {visible.map((entry) => (
              <li key={entry.line.id} className={styles.lineCard}>
                <div className={styles.lineFacts}>
                  {(entry.exact[0] !== undefined || entry.likely[0] !== undefined) && <Checkbox checked={selected.includes(entry.line.id)} label={strings.financeBankSelectLine(entry.line.counterpartyName ?? strings.financeBankNoCounterparty)} onChange={(checked) => setSelected((current) => checked ? [...current, entry.line.id] : current.filter((id) => id !== entry.line.id))} disabled={busy !== null} />}
                  <span className={styles.lineDay}>
                    {dayLabel(entry.line.bookedOn, "—")}
                  </span>
                  <span className={styles.lineWho}>
                    {entry.line.counterpartyName ??
                      strings.financeBankNoCounterparty}
                    {entry.line.counterpartyIban !== null && (
                      <span className={styles.subtle}>
                        {entry.line.counterpartyIban}
                      </span>
                    )}
                  </span>
                  {entry.line.remittance !== null &&
                    entry.line.remittance !== "" && (
                      <span className={styles.lineRemittance}>
                        {entry.line.remittance}
                      </span>
                    )}
                  <span
                    className={cx(
                      styles.lineAmount,
                      entry.line.amountCents < 0 && styles.lineAmountOut,
                    )}
                  >
                    {amountLabel(entry.line.amountCents, entry.line.currency)}
                  </span>
                </div>

                <div className={styles.lineGuesses}>
                  {entry.exact.map((exact) => (
                    <div
                      key={`exact-${exact.invoiceId}`}
                      className={styles.guess}
                    >
                      <span className={styles.guessNumber}>
                        {exact.number}
                        <Badge tone="success">
                          {strings.financeBankCertain}
                        </Badge>
                      </span>
                      <span className={styles.guessWhy}>
                        {strings.financeBankWhyNumberQuoted} ·{" "}
                        {strings.financeBankWhyWholeAmount}
                      </span>
                      <Button
                        size="sm"
                        disabled={busy !== null}
                        onClick={() => void match(entry.line, exact.invoiceId)}
                      >
                        {strings.financeBankThisOne}
                      </Button>
                    </div>
                  ))}

                  {entry.likely.map((likely) => (
                    <Guess
                      key={`likely-${likely.invoiceId}`}
                      likely={likely}
                      currency={entry.line.currency}
                      disabled={busy !== null}
                      onPick={() =>
                        void match(entry.line, likely.invoiceId, likely.ruleId)
                      }
                    />
                  ))}

                  {entry.exact.length === 0 && entry.likely.length === 0 && (
                    <p className={styles.guessNone}>
                      {strings.financeBankNoGuess}
                    </p>
                  )}

                  <div className={styles.rowActions}>
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={busy !== null}
                      onClick={() => void ignore(entry.line)}
                    >
                      {strings.financeBankNotOurs}
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={busy !== null}
                      onClick={() => {
                        setPickError(null);
                        setPicking(entry.line);
                      }}
                    >
                      {strings.financeBankPickInvoice}
                    </Button>
                  </div>
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>

      {settled.length > 0 && (
        <Settled
          title={strings.financeBankSettledTitle}
          tableLabel={strings.financeBankSettledTable}
          note={strings.financeBankSettledNote}
          lines={settled}
          busy={busy}
          undoLabel={strings.financeBankUndoMatch}
          onUndo={(line) => void unmatch(line)}
        />
      )}

      {setAside.length > 0 && (
        <Settled
          title={strings.financeBankSetAsideTitle}
          tableLabel={strings.financeBankSetAsideTable}
          note={strings.financeBankSetAsideNote}
          lines={setAside}
          busy={busy}
          undoLabel={strings.financeBankUndoIgnore}
          onUndo={(line) => void unignore(line)}
        />
      )}

      {picking !== null && (
        <InvoicePicker
          line={picking}
          busy={busy !== null}
          error={pickError}
          onClose={() => {
            setPicking(null);
            setPickError(null);
          }}
          onPick={(invoice: BillingInvoiceSummary) =>
            void match(picking, invoice.id)
          }
        />
      )}
    </div>
  );
}

/** One ranked guess, with every reason behind it spelled out. */
function Guess({
  likely,
  currency,
  disabled,
  onPick,
}: {
  likely: LikelyMatch;
  /** The line's currency: what the evidence's own amounts are counted in. */
  currency: string;
  disabled: boolean;
  onPick: () => void;
}) {
  // A token this client has not learned yet is dropped rather than printed
  // raw — see `evidenceLabel`.
  const why = likely.evidence
    .map((evidence) => evidenceLabel(evidence, currency))
    .filter((sentence): sentence is string => sentence !== null);
  return (
    <div className={styles.guess}>
      <span className={styles.guessNumber}>
        {likely.number}
        <span className={styles.subtle}>
          {strings.financeBankStillOwedIs(
            amountLabel(likely.outstandingCents, currency),
          )}
        </span>
      </span>
      <span className={styles.guessWhy}>{why.join(" · ")}</span>
      <Button variant="ghost" size="sm" disabled={disabled} onClick={onPick}>
        {strings.financeBankThisOne}
      </Button>
    </div>
  );
}

/** A list of lines nobody has to look at again, and the undo beside each. */
function Settled({
  title,
  tableLabel,
  note,
  lines,
  busy,
  undoLabel,
  onUndo,
}: {
  title: string;
  /** What the rows are, for the table's own name. Deliberately not `title`:
   *  the heading says which part of the pile this is, and two tables on one
   *  screen have to be told apart by something a screen reader can hear. */
  tableLabel: string;
  note: string;
  lines: BankLine[];
  busy: string | null;
  undoLabel: string;
  onUndo: (line: BankLine) => void;
}) {
  return (
    <section className={styles.section}>
      <h2 className={styles.sectionTitle}>{title}</h2>
      <p className={styles.sectionNote}>{note}</p>
      <Table label={tableLabel}>
        <thead>
          <tr>
            <Th>{strings.financeBankBookedOn}</Th>
            <Th>{strings.financeBankCounterparty}</Th>
            <Th>{strings.financeBankRemittance}</Th>
            <Th numeric>{strings.financeGross}</Th>
            <Th>{strings.financeStatus}</Th>
            <Th hideLabel>{strings.financeActions}</Th>
          </tr>
        </thead>
        <tbody>
          {lines.map((line) => (
            <tr key={line.id}>
              <Td>{dayLabel(line.bookedOn, "—")}</Td>
              <Td>
                {line.counterpartyName ?? strings.financeBankNoCounterparty}
              </Td>
              <Td className={styles.muted}>
                {line.remittance ?? ""}
                {line.ignoredReason !== null && line.ignoredReason !== "" && (
                  <span className={styles.subtle}>{line.ignoredReason}</span>
                )}
              </Td>
              <Td numeric>{amountLabel(line.amountCents, line.currency)}</Td>
              <Td>
                <Badge tone={BADGE_TONE[lineStatusTone(line.status)]}>
                  {lineStatusLabel(line.status)}
                </Badge>
              </Td>
              <Td>
                <div className={styles.rowActions}>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={busy !== null}
                    onClick={() => onUndo(line)}
                  >
                    {undoLabel}
                  </Button>
                </div>
              </Td>
            </tr>
          ))}
        </tbody>
      </Table>
    </section>
  );
}
