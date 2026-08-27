// Account settings, opened from the account menu, laid out as a two-pane
// preferences panel (section nav + content) in the spirit of Gmail / Outlook
// settings. Sections: General (signature + vacation), Filters & rules, and —
// for admins — the tenant Organization footer. General/Organization save via
// the footer button; Filters persist themselves.
import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import {
  Bell,
  Building2,
  KeyRound,
  PenLine,
  SlidersHorizontal,
  Users,
  X,
} from "lucide-react";

import { strings } from "../i18n";
import {
  Button,
  Field,
  IconButton,
  Input,
  Modal,
  Spinner,
  Toggle,
} from "../ds";
import { useJmapClient } from "../jmap";
import { RichTextEditor } from "../mail/components/RichTextEditor";
import { AppPasswordsSection } from "./AppPasswordsSection";
import { FiltersSection } from "./FiltersSection";
import { PushSection } from "./PushSection";
import { SharingSection } from "./SharingSection";
import styles from "./SettingsModal.module.css";

interface SettingsModalProps {
  isAdmin: boolean;
  onClose: () => void;
}

type Tab =
  | "general"
  | "filters"
  | "sharing"
  | "notifications"
  | "appPasswords"
  | "org";

export function SettingsModal({ isAdmin, onClose }: SettingsModalProps) {
  const client = useJmapClient();
  const [tab, setTab] = useState<Tab>("general");
  const [loaded, setLoaded] = useState(false);
  const [signature, setSignature] = useState("");
  const [orgFooter, setOrgFooter] = useState("");
  const [oooEnabled, setOooEnabled] = useState(false);
  const [oooSubject, setOooSubject] = useState("");
  const [oooMessage, setOooMessage] = useState("");
  // Days, `YYYY-MM-DD`, exactly as a date input holds them; "" is an open end.
  const [oooFrom, setOooFrom] = useState("");
  const [oooTo, setOooTo] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    client
      .mailSettings()
      .then((s) => {
        if (!live) return;
        setSignature(s.signature);
        setOrgFooter(s.orgFooter);
        setOooEnabled(s.outOfOffice.enabled);
        setOooSubject(s.outOfOffice.subject);
        setOooMessage(s.outOfOffice.message);
        setOooFrom(s.outOfOffice.from ?? "");
        setOooTo(s.outOfOffice.to ?? "");
        setLoaded(true);
      })
      .catch(() => {
        if (live) setError(strings.settingsLoadError);
      });
    return () => {
      live = false;
    };
  }, [client]);

  async function save() {
    if (oooEnabled && oooMessage.trim() === "") {
      setError(strings.settingsOooNeedsMessage);
      setTab("general");
      return;
    }
    // Caught here as well as on the server: a backwards window would be
    // refused with a generic save error, and the person who typed it would be
    // looking at the two fields that caused it without being told so.
    if (oooFrom !== "" && oooTo !== "" && oooTo < oooFrom) {
      setError(strings.settingsOooBadWindow);
      setTab("general");
      return;
    }
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      await client.setSignature(signature);
      await client.setOutOfOffice(
        oooEnabled,
        oooSubject,
        oooMessage,
        oooFrom === "" ? null : oooFrom,
        oooTo === "" ? null : oooTo,
      );
      if (isAdmin) await client.setOrgFooter(orgFooter);
      setNote(strings.settingsSaved);
    } catch {
      setError(strings.settingsSaveError);
    } finally {
      setBusy(false);
    }
  }

  const nav: { key: Tab; label: string; icon: ReactNode }[] = [
    {
      key: "general",
      label: strings.settingsTabGeneral,
      icon: <PenLine size={16} />,
    },
    {
      key: "filters",
      label: strings.settingsFilters,
      icon: <SlidersHorizontal size={16} />,
    },
    {
      key: "sharing",
      label: strings.settingsSharing,
      icon: <Users size={16} />,
    },
    {
      key: "notifications",
      label: strings.settingsNotifications,
      icon: <Bell size={16} />,
    },
    {
      key: "appPasswords",
      label: strings.settingsAppPasswords,
      icon: <KeyRound size={16} />,
    },
    ...(isAdmin
      ? [
          {
            key: "org" as Tab,
            label: strings.settingsTabOrg,
            icon: <Building2 size={16} />,
          },
        ]
      : []),
  ];

  return (
    <Modal
      title={strings.settingsTitle}
      onClose={onClose}
      icon={<SlidersHorizontal size={17} />}
      wide
      tall="page"
      actions={
        <IconButton label={strings.userClose} icon={<X />} onClick={onClose} />
      }
      footer={
        <>
          <span className={styles.footMsg}>
            {note !== null && <span className={styles.footOk}>{note}</span>}
            {error !== null && (
              <span className={styles.footErr} role="alert">
                {error}
              </span>
            )}
          </span>
          <Button variant="ghost" onClick={onClose}>
            {strings.userClose}
          </Button>
          <Button onClick={() => void save()} disabled={busy || !loaded}>
            {busy ? <Spinner size={16} /> : strings.settingsSave}
          </Button>
        </>
      }
    >
      {!loaded && error === null ? (
        <div className={styles.loading}>
          <Spinner size={24} />
        </div>
      ) : (
        <div className={styles.body}>
          <nav className={styles.nav} aria-label={strings.settingsTitle}>
            {nav.map((n) => (
              <button
                key={n.key}
                type="button"
                className={tab === n.key ? styles.navItemOn : styles.navItem}
                onClick={() => setTab(n.key)}
                aria-current={tab === n.key}
              >
                <span className={styles.navIcon}>{n.icon}</span>
                <span>{n.label}</span>
              </button>
            ))}
          </nav>

          <div className={styles.content}>
            {tab === "general" && (
              <>
                <section className={styles.section}>
                  <h3 className={styles.sectionTitle}>
                    {strings.settingsSignature}
                  </h3>
                  <p className={styles.sectionDesc}>
                    {strings.settingsSignatureHint}
                  </p>
                  <div className={styles.editorCard}>
                    <RichTextEditor
                      initialHtml={signature}
                      onChange={setSignature}
                      placeholder={strings.settingsSignatureHint}
                    />
                  </div>
                </section>

                <section className={styles.section}>
                  <h3 className={styles.sectionTitle}>
                    {strings.settingsOutOfOffice}
                  </h3>
                  <div className={styles.oooCard}>
                    {/* Was a bare checkbox in a span, with the words beside
                          it bound to nothing and the hint under it read by
                          nobody: announced as "checkbox, not checked" over a
                          setting that is on or off. */}
                    <Toggle
                      checked={oooEnabled}
                      onChange={setOooEnabled}
                      label={strings.settingsOooToggle}
                      hint={strings.settingsOutOfOfficeHint}
                      layout="row"
                    />
                    {oooEnabled && (
                      <>
                        {/* Labelled rather than placeheld: a placeholder is
                              gone the moment you type into it, and it was the
                              only thing naming either control. */}
                        <Field label={strings.settingsOooSubjectPlaceholder}>
                          {(control) => (
                            <Input
                              {...control}
                              value={oooSubject}
                              onChange={(e) => setOooSubject(e.target.value)}
                            />
                          )}
                        </Field>
                        <Field label={strings.settingsOooMessagePlaceholder}>
                          {({ id, "aria-describedby": describedBy }) => (
                            // Not an `Input`: the design system still has no
                            // multi-line control, which is flagged for the
                            // wave review rather than invented here.
                            <textarea
                              id={id}
                              aria-describedby={describedBy}
                              className={styles.textarea}
                              rows={4}
                              value={oooMessage}
                              onChange={(e) => setOooMessage(e.target.value)}
                            />
                          )}
                        </Field>
                        {/* The dates are what makes this a schedule rather
                              than a switch: you set it the evening before you
                              leave, and it stops on its own. Both are
                              optional, and blank keeps the old behaviour —
                              on now, until you turn it off — so the hint says
                              that rather than leaving it to be discovered. */}
                        <div className={styles.oooDates}>
                          <Field label={strings.settingsOooFrom}>
                            {(control) => (
                              <Input
                                {...control}
                                type="date"
                                value={oooFrom}
                                onChange={(e) => setOooFrom(e.target.value)}
                              />
                            )}
                          </Field>
                          <Field
                            label={strings.settingsOooTo}
                            // The browser rejects an end before the start
                            // before a save is ever attempted.
                            hint={strings.settingsOooDatesHint}
                          >
                            {(control) => (
                              <Input
                                {...control}
                                type="date"
                                min={oooFrom === "" ? undefined : oooFrom}
                                value={oooTo}
                                onChange={(e) => setOooTo(e.target.value)}
                              />
                            )}
                          </Field>
                        </div>
                      </>
                    )}
                  </div>
                </section>
              </>
            )}

            {tab === "filters" && (
              <section className={styles.section}>
                <h3 className={styles.sectionTitle}>
                  {strings.settingsFilters}
                </h3>
                <p className={styles.sectionDesc}>
                  {strings.settingsFiltersHint}
                </p>
                <FiltersSection />
              </section>
            )}

            {tab === "sharing" && (
              <section className={styles.section}>
                <h3 className={styles.sectionTitle}>
                  {strings.settingsSharing}
                </h3>
                <p className={styles.sectionDesc}>
                  {strings.settingsSharingHint}
                </p>
                <SharingSection />
              </section>
            )}

            {tab === "notifications" && (
              <section className={styles.section}>
                <h3 className={styles.sectionTitle}>
                  {strings.settingsNotifications}
                </h3>
                <p className={styles.sectionDesc}>
                  {strings.settingsNotificationsHint}
                </p>
                <PushSection />
              </section>
            )}

            {tab === "appPasswords" && (
              <section className={styles.section}>
                <h3 className={styles.sectionTitle}>
                  {strings.settingsAppPasswords}
                </h3>
                <p className={styles.sectionDesc}>
                  {strings.settingsAppPasswordsHint}
                </p>
                <AppPasswordsSection />
              </section>
            )}

            {tab === "org" && isAdmin && (
              <section className={styles.section}>
                <h3 className={styles.sectionTitle}>
                  {strings.settingsOrgFooter}
                </h3>
                <p className={styles.sectionDesc}>
                  {strings.settingsOrgFooterHint}
                </p>
                <div className={styles.editorCard}>
                  <RichTextEditor
                    initialHtml={orgFooter}
                    onChange={setOrgFooter}
                    placeholder={strings.settingsOrgFooterPlaceholder}
                  />
                </div>
              </section>
            )}
          </div>
        </div>
      )}
    </Modal>
  );
}
