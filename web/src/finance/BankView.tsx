// The Bank tab: what has been imported, and the door that imports more.
//
// One statement is one file somebody uploaded, and the list is the month-by-
// month record of that — which account, which reader understood it, how many
// transactions it holds and who staged it. It is deliberately **not** narrowed
// by user: a statement is the company's, and the point of importing a month is
// that it is imported once.
//
// The list is a record, not a workspace. Everything a person *does* with the
// lines happens on the reconciliation screen next door, so this screen's only
// verb is Import and its empty state says what the tab is for rather than
// listing what it cannot do yet.
import { useCallback, useEffect, useState } from "react";
import { Landmark, Upload } from "lucide-react";

import { Button, Spinner, Table, Td, Th, Toolbar, ToolbarSpacer } from "../ds";
import { strings } from "../i18n";
import { financeMessage, useFinanceApi } from "./api";
import { BankImportDialog } from "./BankImportDialog";
import { amountLabel, dayLabel, momentLabel, sourceLabel } from "./format";
import { EmptyState, ErrorBanner } from "./parts";
import type { BankStatement } from "./types";
import styles from "./FinanceModule.module.css";

export function BankView({ onImported }: { onImported: () => void }) {
  const api = useFinanceApi();
  const [statements, setStatements] = useState<BankStatement[]>([]);
  const [importing, setImporting] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [staged, setStaged] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);

  const reload = useCallback(() => setRevision((r) => r + 1), []);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const list = await api.bankStatements();
        if (live) {
          setStatements(list);
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
  }, [api, revision]);

  return (
    <div className={styles.page}>
      <Toolbar label={strings.financeTabBank}>
        <ToolbarSpacer />
        {loading && <Spinner size={16} />}
        <Button onClick={() => setImporting(true)}>
          <Upload size={16} /> {strings.financeBankImportStatement}
        </Button>
      </Toolbar>

      {error !== null && <ErrorBanner message={error} />}
      {/* What the commit actually did, in the server's own counts. The
          duplicates a second upload of an overlapping month skipped are the
          number a person is looking for, and a bare "imported" would hide
          them. */}
      {staged !== null && <p className={styles.notice}>{staged}</p>}

      {statements.length === 0 && !loading ? (
        <EmptyState
          Icon={Landmark}
          title={strings.financeBankEmptyTitle}
          body={strings.financeBankEmptyBody}
          cta={strings.financeBankImportStatement}
          onCta={() => setImporting(true)}
        />
      ) : (
        <Table label={strings.financeStatementsTable}>
          <thead>
            <tr>
              <Th>{strings.financeBankPeriod}</Th>
              <Th>{strings.financeBankAccount}</Th>
              <Th>{strings.financeBankFormat}</Th>
              <Th numeric>{strings.financeBankLines}</Th>
              <Th numeric>{strings.financeBankClosingBalance}</Th>
              <Th>{strings.financeBankImportedAt}</Th>
            </tr>
          </thead>
          <tbody>
            {statements.map((statement) => (
              <tr key={statement.id}>
                <Td>
                  {dayLabel(statement.fromDate, "—")} –{" "}
                  {dayLabel(statement.toDate, "—")}
                  {statement.statementRef !== null &&
                    statement.statementRef !== "" && (
                      <span className={styles.subtle}>
                        {statement.statementRef}
                      </span>
                    )}
                </Td>
                <Td>
                  {statement.accountIban}
                  <span className={styles.subtle}>{statement.currency}</span>
                </Td>
                <Td className={styles.muted}>
                  {sourceLabel(statement.source)}
                </Td>
                <Td numeric>{statement.lineCount}</Td>
                <Td numeric>
                  {statement.closingBalanceCents === null ? (
                    <span className={styles.muted}>{strings.financeNoVat}</span>
                  ) : (
                    amountLabel(
                      statement.closingBalanceCents,
                      statement.currency,
                    )
                  )}
                </Td>
                <Td className={styles.muted}>
                  {momentLabel(statement.importedAt)}
                </Td>
              </tr>
            ))}
          </tbody>
        </Table>
      )}

      {importing && (
        <BankImportDialog
          onClose={() => setImporting(false)}
          onImported={(report) => {
            setImporting(false);
            setStaged(
              strings.financeBankStaged(
                report.counts.staged ?? 0,
                report.counts.duplicates ?? 0,
              ),
            );
            reload();
            // The pile next door just grew; the module's counter is what tells
            // the reconciliation screen so, without either tab reloading the
            // other's data behind its back.
            onImported();
          }}
        />
      )}
    </div>
  );
}
