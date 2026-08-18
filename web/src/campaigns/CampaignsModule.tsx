// The Campaigns module (alo Campaigns, ADR 0044, wave C1) — the audience
// screen: the question, the count that moves as it is refined, and the people
// the count leaves out, named with the reason.
//
// **Nothing on this screen sends anything, and it says so.** The sending
// identity needs a second IP that has to be bought (ADR 0044 §1), so a button
// that promised a send would be a promise the product cannot keep. The footnote
// at the bottom is not an apology; it is the difference between a screen that
// is honest about what it is for and one that a colleague misreads.
//
// What this file owns is the chrome and the saved questions. The question
// itself is `QuestionBar`, the count is `TallyLine`, the people are
// `AudienceTable`, and every number on it was computed by the server.
//
// **Two views, because they answer two different questions** (wave C3.6): *who
// would this reach* and *what would they actually get*. They are tabs rather
// than two modules in the rail because a colleague moves between them while
// deciding one thing — and because the second is meaningless without the first:
// the letter is previewed against somebody from the audience.
import { useState } from "react";
import { Megaphone } from "lucide-react";

import { Button, Select, Spinner, useDialogs } from "../ds";
import { strings } from "../i18n";
import { AudienceTable } from "./AudienceTable";
import { LetterPreview } from "./LetterPreview";
import { QuestionBar } from "./QuestionBar";
import { TallyLine } from "./TallyLine";
import { campaignsMessage, useCampaignsApi } from "./api";
import { useAudience } from "./useAudience";
import { useSegments } from "./useSegments";
import { NO_CONDITIONS, type SegmentConditions } from "./types";
import styles from "./CampaignsModule.module.css";

/** Which question the screen is answering. */
type View = "audience" | "letters";

export function CampaignsModule() {
  const api = useCampaignsApi();
  const dialogs = useDialogs();
  const segments = useSegments();

  // The question on screen, and the countries box exactly as typed. Two states
  // rather than one because a half-typed code ("B") is not a country and must
  // not be thrown away by the parse that produces the conditions.
  const [conditions, setConditions] = useState<SegmentConditions>(NO_CONDITIONS);
  const [countries, setCountries] = useState("");
  const [openSegment, setOpenSegment] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<View>("audience");

  const audience = useAudience(conditions);

  /** Opens a saved question — which is only ever loading its conditions. The
   *  count is then asked exactly as it is for an unsaved one, so a saved
   *  segment cannot mean something different from the draft it came from. */
  function open(id: string) {
    setOpenSegment(id);
    const saved = segments.segments.find((segment) => segment.id === id);
    const asked = saved?.conditions ?? NO_CONDITIONS;
    setConditions(asked);
    setCountries(asked.countries.join(", "));
  }

  function askAboutEveryone() {
    setOpenSegment("");
    setConditions(NO_CONDITIONS);
    setCountries("");
  }

  function save() {
    void (async () => {
      const name = (
        await dialogs.prompt({
          title: strings.campaignsSaveSegment,
          message: strings.campaignsSegmentNamePrompt,
          placeholder: strings.campaignsSegmentNamePlaceholder,
        })
      )?.trim();
      if (name === undefined || name === "") return;
      try {
        const saved = await api.createSegment({ name, conditions });
        setError(null);
        setOpenSegment(saved.id);
        segments.reload();
      } catch (err) {
        setError(campaignsMessage(err, strings.campaignsSaveFailed));
      }
    })();
  }

  function forget() {
    const saved = segments.segments.find((segment) => segment.id === openSegment);
    if (saved === undefined) return;
    void (async () => {
      const confirmed = await dialogs.confirm({
        title: strings.campaignsDeleteSegment,
        message: strings.campaignsDeleteSegmentConfirm(saved.name),
        confirmLabel: strings.campaignsDeleteSegment,
        danger: true,
      });
      if (!confirmed) return;
      try {
        await api.deleteSegment(saved.id);
        setError(null);
        askAboutEveryone();
        segments.reload();
      } catch (err) {
        setError(campaignsMessage(err, strings.campaignsDeleteFailed));
      }
    })();
  }

  const banner = error ?? audience.error ?? segments.error;
  const empty =
    !audience.loading && audience.people.length === 0 && audience.tally?.matched === 0;

  return (
    <div className={styles.campaigns}>
      <header className={styles.header}>
        <div>
          <h1 className={styles.title}>
            {view === "audience" ? strings.campaignsTitle : strings.campaignsLettersTitle}
          </h1>
          <p className={styles.subtitle}>
            {view === "audience" ? strings.campaignsSubtitle : strings.campaignsLettersSubtitle}
          </p>
        </div>
        {view === "audience" && (
          <div className={styles.savedQuestions}>
            <Select
              aria-label={strings.campaignsSegmentsLabel}
              value={openSegment}
              onChange={(e) => open(e.target.value)}
            >
              <option value="">{strings.campaignsEveryone}</option>
              {segments.segments.map((segment) => (
                <option key={segment.id} value={segment.id}>
                  {segment.name}
                </option>
              ))}
            </Select>
            <Button variant="secondary" onClick={save}>
              {strings.campaignsSaveSegment}
            </Button>
            {openSegment !== "" && (
              <Button variant="ghost" onClick={forget}>
                {strings.campaignsDeleteSegment}
              </Button>
            )}
          </div>
        )}
      </header>

      <div className={styles.views} role="tablist" aria-label={strings.campaignsViewsLabel}>
        {(["audience", "letters"] as const).map((tab) => (
          <button
            key={tab}
            type="button"
            role="tab"
            id={`campaigns-tab-${tab}`}
            aria-selected={view === tab}
            aria-controls="campaigns-panel"
            className={view === tab ? styles.viewOpen : styles.view}
            onClick={() => setView(tab)}
          >
            {tab === "audience" ? strings.campaignsTabAudience : strings.campaignsTabLetters}
          </button>
        ))}
      </div>

      <div
        className={styles.panel}
        id="campaigns-panel"
        role="tabpanel"
        aria-labelledby={`campaigns-tab-${view}`}
      >
        {view === "letters" ? (
          <LetterPreview />
        ) : (
          <AudienceView
            banner={banner}
            countries={countries}
            conditions={conditions}
            setCountries={setCountries}
            onConditions={(asked) => {
              // Editing a saved question makes it a draft again, rather than
              // silently changing the thing a colleague saved under that name.
              setOpenSegment("");
              setConditions(asked);
            }}
            audience={audience}
            empty={empty}
          />
        )}
      </div>

      <p className={styles.footnote}>{strings.campaignsNothingSentYet}</p>
    </div>
  );
}

/** The audience half of the screen: the question, the count, and the people —
 *  unchanged from wave C1.5 and lifted into its own component only so the two
 *  views read as two views in the file that switches between them. */
function AudienceView({
  banner,
  countries,
  conditions,
  setCountries,
  onConditions,
  audience,
  empty,
}: {
  banner: string | null;
  countries: string;
  conditions: SegmentConditions;
  setCountries: (raw: string) => void;
  onConditions: (asked: SegmentConditions) => void;
  audience: ReturnType<typeof useAudience>;
  empty: boolean;
}) {
  return (
    <>
      {banner !== null && (
        <p className={styles.error} role="alert">
          {banner}
        </p>
      )}

      <QuestionBar
        countries={countries}
        conditions={conditions}
        onCountries={setCountries}
        onConditions={onConditions}
      />

      <TallyLine tally={audience.tally} loading={audience.loading} />

      {empty ? (
        <div className={styles.empty}>
          <span className={styles.emptyArt} aria-hidden="true">
            <Megaphone size={38} />
          </span>
          <h2 className={styles.emptyTitle}>{strings.campaignsEmptyTitle}</h2>
          <p className={styles.emptyBody}>{strings.campaignsEmptyBody}</p>
        </div>
      ) : (
        <div className={styles.list}>
          <AudienceTable people={audience.people} />
          {audience.hasMore && (
            <div className={styles.more}>
              <Button variant="secondary" onClick={audience.more} disabled={audience.loading}>
                {audience.loading ? <Spinner /> : strings.campaignsMore}
              </Button>
            </div>
          )}
        </div>
      )}
    </>
  );
}
