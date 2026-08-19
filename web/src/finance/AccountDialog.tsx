// Adding an account to the chart, and changing one that is already in it.
//
// Three things this form does that a generic CRUD dialog would not.
//
// **It explains what a role is, in the sentence a role means.** `ar` is a word
// on the wire; "trade receivables — what customers owe us" is what the person
// choosing it needs to read. The select therefore offers the sentences, and the
// hint under it says the one thing a person must know before touching this
// field: the posting rules find their account by role, so clearing one stops
// documents booking, and a code is safe to change while a role is not.
//
// **It never invents a type.** A new account's kind is chosen, not defaulted to
// "expense" — an asset filed as a cost is a wrong balance sheet that looks like
// a right one — so the submit stays disabled until it is said.
//
// **Deleting is offered only where it is possible.** A seeded account and one
// that carries a posting are both refused by the server (`409`), and offering a
// button whose only outcome is a refusal would be advertising a door that does
// not open. What is offered instead, on both, is retiring the account — which
// is what a tenant who has stopped using one actually wants.
import { useState } from "react";
import { BookOpen } from "lucide-react";

import { Field, Input, Select } from "../ds";
import { strings } from "../i18n";
import { accountRoleLabel } from "./format";
import { DialogFrame } from "./parts";
import type {
  AccountDraft,
  AccountRole,
  AccountType,
  ChartAccount,
} from "./types";
import styles from "./FinanceModule.module.css";

/** The five kinds, in the order a chart is laid out. */
const TYPES: AccountType[] = [
  "asset",
  "liability",
  "equity",
  "income",
  "expense",
];

/** Every posting-rule job, in the order the default chart introduces them —
 *  the server's own order (`AccountRole::ALL`), so the two lists read the
 *  same. */
const ROLES: AccountRole[] = [
  "bank",
  "cash",
  "ar",
  "vat_input",
  "suspense",
  "ap",
  "vat_output",
  "employee_payable",
  "opening_balance",
  "revenue",
  "expense_default",
  "rounding",
  "fx_diff",
];

export function AccountDialog({
  account,
  busy,
  error,
  onClose,
  onSave,
  onDelete,
}: {
  /** The account being changed, or `null` when one is being added. */
  account: ChartAccount | null;
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onSave: (draft: AccountDraft) => void;
  onDelete: (() => void) | undefined;
}) {
  const [code, setCode] = useState(account?.code ?? "");
  const [name, setName] = useState(account?.name ?? "");
  // Empty means "not said yet" on a new account. It is never pre-filled: an
  // asset filed as an expense is a wrong balance sheet that looks like a right
  // one, and a default would make that the quiet outcome of not reading.
  const [kind, setKind] = useState<AccountType | "">(account?.type ?? "");
  const [role, setRole] = useState<AccountRole | "">(account?.role ?? "");
  const [active, setActive] = useState(account?.active ?? true);

  const editing = account !== null;
  const canSubmit = code.trim() !== "" && name.trim() !== "" && kind !== "";

  function submit() {
    if (kind === "") return;
    const draft: AccountDraft = {
      code: code.trim(),
      name: name.trim(),
      type: kind,
      role,
    };
    // `active` is a field of the edit only: an account somebody just added is
    // one they mean to use, and a switch to turn it on would exist for nobody.
    if (editing) draft.active = active;
    onSave(draft);
  }

  return (
    <DialogFrame
      Icon={BookOpen}
      title={
        editing
          ? strings.financeAccountEditTitle
          : strings.financeAccountNewTitle
      }
      subtitle={
        editing ? strings.financeAccountEditBody : strings.financeAccountNewBody
      }
      error={error}
      busy={busy}
      canSubmit={canSubmit}
      submitLabel={editing ? strings.financeSave : strings.financeAccountAdd}
      extraAction={
        onDelete === undefined
          ? undefined
          : { label: strings.financeAccountDelete, onClick: onDelete }
      }
      onClose={onClose}
      onSubmit={submit}
    >
      <div className={styles.row}>
        <Field
          label={strings.financeAccountCode}
          hint={strings.financeAccountCodeHint}
        >
          {(control) => (
            <Input
              {...control}
              value={code}
              onChange={(e) => setCode(e.target.value)}
              maxLength={20}
              required
              autoFocus={!editing}
            />
          )}
        </Field>
        <Field label={strings.financeAccountName}>
          {(control) => (
            <Input
              {...control}
              value={name}
              onChange={(e) => setName(e.target.value)}
              maxLength={120}
              required
            />
          )}
        </Field>
      </div>

      <Field
        label={strings.financeAccountType}
        hint={strings.financeAccountTypeHint}
      >
        {(control) => (
          // A prompt, not an answer: `Select` disables it on a required field,
          // which is also the only spelling the browser's own required check
          // understands — a sentinel value passes it.
          <Select
            {...control}
            fullWidth
            placeholder={strings.financeAccountTypeUnset}
            value={kind}
            onChange={(e) => setKind(e.target.value as AccountType | "")}
            required
          >
            {TYPES.map((option) => (
              <option key={option} value={option}>
                {typeSentence(option)}
              </option>
            ))}
          </Select>
        )}
      </Field>

      <Field
        label={strings.financeAccountRole}
        hint={strings.financeAccountRoleHint}
      >
        {(control) => (
          // "No job" is a real answer here — most accounts have none — so it
          // stays choosable.
          <Select
            {...control}
            fullWidth
            placeholder={strings.financeRoleNone}
            value={role}
            onChange={(e) => setRole(e.target.value as AccountRole | "")}
          >
            {ROLES.map((option) => (
              <option key={option} value={option}>
                {accountRoleLabel(option)}
              </option>
            ))}
          </Select>
        )}
      </Field>

      {editing && (
        <Field
          label={strings.financeAccountActive}
          hint={strings.financeAccountActiveHint}
        >
          {(control) => (
            <Select
              {...control}
              fullWidth
              value={active ? "yes" : "no"}
              onChange={(e) => setActive(e.target.value === "yes")}
            >
              <option value="yes">{strings.financeAccountInUse}</option>
              <option value="no">{strings.financeAccountRetired}</option>
            </Select>
          )}
        </Field>
      )}

      {/* What a seeded account is, said where somebody is about to wonder why
          they cannot delete it. */}
      {account?.system === true && (
        <p className={styles.hint}>{strings.financeAccountSystemNote}</p>
      )}
    </DialogFrame>
  );
}

/** The kind, said as what it holds rather than as its category name: "what the
 *  business owns or is owed" is the question somebody filing an account is
 *  actually answering. */
function typeSentence(kind: AccountType): string {
  switch (kind) {
    case "asset":
      return strings.financeAccountTypeAssetLong;
    case "liability":
      return strings.financeAccountTypeLiabilityLong;
    case "equity":
      return strings.financeAccountTypeEquityLong;
    case "income":
      return strings.financeAccountTypeIncomeLong;
    default:
      return strings.financeAccountTypeExpenseLong;
  }
}
