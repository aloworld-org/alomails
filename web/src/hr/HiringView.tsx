// The hiring screen: which round is on screen, what state it is in, and the
// board of the people who applied for it.
//
// It owns the reads, so the board stays a board (`HiringBoard.tsx` draws
// columns and cards and knows nothing about HTTP). Three decisions live here:
//
//   - **The round on screen is in the address** (`?opening=`), like CRM's open
//     deal and for the same reason: a link to a hiring round is a link somebody
//     can send, and a reload lands back on the round that was being worked.
//   - **Closed rounds are off the picker until asked for.** A recruiter opening
//     Hiring wants the roles they are hiring for; last year's are a checkbox
//     away, and their board still reads — the applicants stay as the record of
//     what happened (`docs/design/hr.md` § Recruitment-lite).
//   - **The state of a round is a chip, not a mode.** Draft, open and closed
//     each change which acts are offered — publish, close, add a candidate —
//     and an act the server would refuse is never drawn.
import { useCallback, useEffect, useState } from "react";
import { Briefcase, Plus } from "lucide-react";
import { useNavigate, useSearchParams } from "react-router-dom";

import {
  Button,
  Checkbox,
  Field,
  Select,
  Spinner,
  Toolbar,
  useDialogs,
} from "../ds";
import { strings } from "../i18n";
import { hrMessage, useHrApi } from "./api";
import { ApplicantDialog } from "./ApplicantDialog";
import { ApplicantDrawer } from "./ApplicantDrawer";
import { HireDialog } from "./HireDialog";
import { HiringBoard } from "./HiringBoard";
import { dayLabel, kindLabel, openingLabel, statusLabel } from "./format";
import { OpeningDialog } from "./OpeningDialog";
import { EmptyState, ErrorBanner, StateBadge } from "./parts";
import type { HrApplicant, HrOpening } from "./types";
import styles from "./hr.module.css";

/** Which form is open, and on what. `null` is the ordinary state. */
type Editing =
  | { kind: "opening"; opening: HrOpening | null }
  | { kind: "applicant"; applicant: HrApplicant | null }
  | { kind: "hire"; applicant: HrApplicant };

export function HiringView() {
  const api = useHrApi();
  const { confirm } = useDialogs();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const [openings, setOpenings] = useState<HrOpening[]>([]);
  const [applicants, setApplicants] = useState<HrApplicant[]>([]);
  const [stages, setStages] = useState<string[]>([]);
  const [includeClosed, setIncludeClosed] = useState(false);
  const [editing, setEditing] = useState<Editing | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  // One counter ties the picker, the board and the drawer together: a move made
  // in the drawer and an application recorded from a column both bump it, and
  // whatever is on screen re-reads rather than drifting from the record.
  const [revision, setRevision] = useState(0);
  const bump = useCallback(() => setRevision((r) => r + 1), []);

  const openingId = searchParams.get("opening");
  const applicantId = searchParams.get("applicant");
  const opening = openings.find((o) => o.id === openingId) ?? null;

  /** Puts a key in the address, or takes it out. Replace, not push: working a
   *  board is not a trail of history entries to press Back through. */
  const setParam = useCallback(
    (key: string, value: string | null) => {
      setSearchParams(
        (params) => {
          const next = new URLSearchParams(params);
          if (value === null) next.delete(key);
          else next.set(key, value);
          return next;
        },
        { replace: true },
      );
    },
    [setSearchParams],
  );

  // The rounds. Selecting the first one when the address names none (or names
  // one this list does not hold — a closed round with the checkbox off) is what
  // makes the screen open on something rather than on a picker.
  //
  // Which round is named is read **inside** the updater rather than from this
  // component's copy of the address, so opening a drawer does not re-read the
  // list of rounds for nothing.
  useEffect(() => {
    let live = true;
    setLoading(true);
    api
      .openings(includeClosed)
      .then((all) => {
        if (!live) return;
        setOpenings(all);
        setError(null);
        setSearchParams(
          (params) => {
            if (all.some((round) => round.id === params.get("opening")))
              return params;
            const next = new URLSearchParams(params);
            const first = all[0]?.id;
            if (first === undefined) next.delete("opening");
            else next.set("opening", first);
            return next;
          },
          { replace: true },
        );
      })
      .catch((err: unknown) => {
        if (live) setError(hrMessage(err, strings.hrLoadFailed));
      })
      .finally(() => {
        if (live) setLoading(false);
      });
    return () => {
      live = false;
    };
  }, [api, includeClosed, revision, setSearchParams]);

  // The board of the round on screen, with the stage vocabulary its columns are
  // drawn from — one read, so a board is never a guess about which stages exist.
  useEffect(() => {
    if (openingId === null) {
      setApplicants([]);
      setStages([]);
      return;
    }
    let live = true;
    api
      .pipeline(openingId)
      .then((pipeline) => {
        if (!live) return;
        setApplicants(pipeline.applicants);
        setStages(pipeline.stages);
        setError(null);
      })
      .catch((err: unknown) => {
        if (live) setError(hrMessage(err, strings.hrLoadFailed));
      });
    return () => {
      live = false;
    };
  }, [api, openingId, revision]);

  /** A card was dropped on a column. The board is redrawn from the server's
   *  answer, so a refusal leaves it exactly as it was. */
  async function move(id: string, stage: string) {
    setError(null);
    try {
      const moved = await api.moveApplicant(id, stage);
      setApplicants((all) =>
        all.map((person) => (person.id === id ? moved : person)),
      );
    } catch (err) {
      setError(hrMessage(err, strings.hrSaveFailed));
      bump();
    }
  }

  /** Publishing says the round is running. Not confirmed: it is undone by
   *  nothing worse than closing the round, and the day it opened is a fact
   *  worth recording rather than a decision worth interrupting. */
  async function publish(id: string) {
    setBusy(true);
    setError(null);
    try {
      await api.publishOpening(id);
      bump();
    } catch (err) {
      setError(hrMessage(err, strings.hrSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  /** Closing IS confirmed: it is terminal, it freezes what the role said, and
   *  no button anywhere reopens it. */
  async function close(round: HrOpening) {
    const sure = await confirm({
      title: strings.hrCloseOpening,
      message: strings.hrCloseConfirm(round.title),
      confirmLabel: strings.hrCloseOpening,
    });
    if (!sure) return;
    setBusy(true);
    setError(null);
    try {
      await api.closeOpening(round.id);
      bump();
    } catch (err) {
      setError(hrMessage(err, strings.hrSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  const canApply = opening !== null && opening.status !== "closed";

  return (
    <div className={styles.page}>
      <Toolbar
        label={strings.hrHiringControls}
        align="end"
        className="px-5 pt-4"
      >
        {openings.length > 0 && (
          <Field label={strings.hrOpening}>
            {(control) => (
              <Select
                {...control}
                className="max-w-[320px]"
                value={opening?.id ?? ""}
                onChange={(e) => {
                  setParam("applicant", null);
                  setParam("opening", e.target.value);
                }}
              >
                {openings.map((round) => (
                  <option key={round.id} value={round.id}>
                    {openingLabel(round)}
                  </option>
                ))}
              </Select>
            )}
          </Field>
        )}
        {opening !== null && (
          <>
            <StateBadge tone={opening.status === "closed" ? "bad" : "info"}>
              {statusLabel(opening.status)}
            </StateBadge>
            <span className={styles.openingFacts}>
              {[
                kindLabel(opening.employmentKind),
                opening.location,
                opening.status === "closed"
                  ? strings.hrClosedOn(dayLabel(opening.closedOn))
                  : opening.openedOn === null
                    ? ""
                    : strings.hrOpenedOn(dayLabel(opening.openedOn)),
              ]
                .filter((part) => part !== "")
                .join(" · ")}
            </span>
          </>
        )}
        <span className="flex-1" />
        {loading && <Spinner size={16} />}
        <Checkbox
          checked={includeClosed}
          onChange={setIncludeClosed}
          label={strings.hrIncludeClosed}
        />
        {opening !== null && opening.status === "draft" && (
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => void publish(opening.id)}
          >
            {strings.hrPublishOpening}
          </Button>
        )}
        {opening !== null && opening.status !== "closed" && (
          <>
            <Button
              variant="ghost"
              disabled={busy}
              onClick={() => setEditing({ kind: "opening", opening })}
            >
              {strings.hrEditOpening}
            </Button>
            <Button
              variant="ghost"
              disabled={busy}
              onClick={() => void close(opening)}
            >
              {strings.hrCloseOpening}
            </Button>
          </>
        )}
        <Button
          icon={<Plus size={15} />}
          onClick={() => setEditing({ kind: "opening", opening: null })}
        >
          {strings.hrNewOpening}
        </Button>
      </Toolbar>

      {error !== null && <ErrorBanner message={error} />}

      {opening === null && !loading ? (
        <EmptyState
          Icon={Briefcase}
          title={strings.hrNoOpeningsTitle}
          body={strings.hrNoOpeningsBody}
          cta={strings.hrNewOpening}
          onCta={() => setEditing({ kind: "opening", opening: null })}
        />
      ) : (
        <>
          {opening !== null && opening.status === "closed" && (
            <p className={styles.notice}>{strings.hrClosedNotice}</p>
          )}
          <HiringBoard
            stages={stages}
            applicants={applicants}
            addStage={canApply ? (stages[0] ?? null) : null}
            onOpen={(id) => setParam("applicant", id)}
            onMove={(id, stage) => void move(id, stage)}
            onAdd={() => setEditing({ kind: "applicant", applicant: null })}
          />
        </>
      )}

      {applicantId !== null && (
        <ApplicantDrawer
          key={applicantId}
          applicantId={applicantId}
          onClose={() => setParam("applicant", null)}
          onChanged={bump}
          onEdit={(applicant) => setEditing({ kind: "applicant", applicant })}
          onHire={(applicant) => setEditing({ kind: "hire", applicant })}
          onGone={() => {
            setParam("applicant", null);
            bump();
          }}
        />
      )}

      {editing?.kind === "opening" && (
        <OpeningDialog
          opening={editing.opening}
          onClose={() => setEditing(null)}
          onSaved={(saved) => {
            setEditing(null);
            // A round written down here is the one to work on next; a
            // correction leaves the screen where it was.
            if (editing.opening === null) setParam("opening", saved.id);
            bump();
          }}
        />
      )}

      {editing?.kind === "applicant" && opening !== null && (
        <ApplicantDialog
          applicant={editing.applicant}
          openingId={opening.id}
          onClose={() => setEditing(null)}
          onSaved={(saved) => {
            setEditing(null);
            // Somebody recorded is somebody to read: the drawer opens on them,
            // which is also where the CV and the notes are.
            if (editing.applicant === null) setParam("applicant", saved.id);
            bump();
          }}
        />
      )}

      {editing?.kind === "hire" && (
        <HireDialog
          applicant={editing.applicant}
          opening={opening}
          onClose={() => setEditing(null)}
          onHired={(employee) => {
            setEditing(null);
            // The confirmation is the colleague themselves: the directory,
            // searched for the name that was just written into it. A sentence
            // saying "done" would be one more thing to believe.
            navigate(`/hr/directory?q=${encodeURIComponent(employee.name)}`);
          }}
        />
      )}
    </div>
  );
}
