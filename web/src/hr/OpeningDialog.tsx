// Write down a role, or correct the one on screen.
//
// Nothing is judged here. A blank title, an over-long team name and an
// employment word this build does not know are all the server's refusals, shown
// as the sentences it wrote — and the form cannot publish or close a round,
// because those are their own audited acts (`docs/design/hr.md` §
// Recruitment-lite).
import { useState } from "react";
import { Briefcase } from "lucide-react";

import { Field, Input, Select } from "../ds";
import { strings } from "../i18n";
import { hrMessage, useHrApi } from "./api";
import { kindLabel } from "./format";
import { DialogFrame } from "./parts";
import { EMPLOYMENT_KINDS, type HrOpening } from "./types";
import styles from "./hr.module.css";

interface Props {
  /** The opening being corrected, or `null` to write one down. */
  opening: HrOpening | null;
  onClose: () => void;
  /** The caller re-reads from the server, so what it saved is passed on only as
   *  the record to select — never as the state of the screen. */
  onSaved: (opening: HrOpening) => void;
}

export function OpeningDialog({ opening, onClose, onSaved }: Props) {
  const api = useHrApi();
  const [title, setTitle] = useState(opening?.title ?? "");
  const [team, setTeam] = useState(opening?.team ?? "");
  const [location, setLocation] = useState(opening?.location ?? "");
  const [kind, setKind] = useState(opening?.employmentKind ?? "permanent");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function save() {
    setBusy(true);
    setError(null);
    try {
      // Only what changed is sent (the module rule billing set): a PATCH that
      // replayed every field would overwrite a colleague's edit with a value
      // this form read minutes ago.
      const draft: {
        title?: string;
        team?: string;
        location?: string;
        employmentKind?: string;
      } = {};
      if (opening === null || title.trim() !== opening.title)
        draft.title = title.trim();
      if (
        opening === null ? team.trim() !== "" : team.trim() !== opening.team
      ) {
        draft.team = team.trim();
      }
      if (
        opening === null
          ? location.trim() !== ""
          : location.trim() !== opening.location
      ) {
        draft.location = location.trim();
      }
      if (opening === null || kind !== opening.employmentKind)
        draft.employmentKind = kind;
      onSaved(
        opening === null
          ? await api.createOpening(draft)
          : await api.updateOpening(opening.id, draft),
      );
    } catch (err) {
      setError(hrMessage(err, strings.hrSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={Briefcase}
      title={opening === null ? strings.hrNewOpening : strings.hrEditOpening}
      subtitle={strings.hrOpeningSubtitle}
      error={error}
      busy={busy}
      canSubmit={title.trim() !== ""}
      submitLabel={opening === null ? strings.hrCreate : strings.hrSave}
      onClose={onClose}
      onSubmit={() => void save()}
    >
      <Field label={strings.hrFieldRole}>
        {(control) => (
          <Input
            {...control}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            autoFocus
            required
          />
        )}
      </Field>

      <div className={styles.row}>
        <Field label={strings.hrFieldTeam}>
          {(control) => (
            <Input
              {...control}
              value={team}
              onChange={(e) => setTeam(e.target.value)}
            />
          )}
        </Field>
        <Field label={strings.hrFieldLocation} hint={strings.hrLocationHint}>
          {(control) => (
            <Input
              {...control}
              value={location}
              onChange={(e) => setLocation(e.target.value)}
            />
          )}
        </Field>
      </div>

      <Field label={strings.hrFieldEmployment}>
        {(control) => (
          <Select
            {...control}
            fullWidth
            value={kind}
            onChange={(e) => setKind(e.target.value)}
          >
            {/* A word an older record carries that this build has dropped stays
                selectable, so editing the team of such an opening cannot
                silently change what it is a contract for. */}
            {(EMPLOYMENT_KINDS as readonly string[]).includes(kind) ? null : (
              <option value={kind}>{kindLabel(kind)}</option>
            )}
            {EMPLOYMENT_KINDS.map((word) => (
              <option key={word} value={word}>
                {kindLabel(word)}
              </option>
            ))}
          </Select>
        )}
      </Field>
    </DialogFrame>
  );
}
