// Account settings, opened from the account menu, laid out as a two-pane
// preferences panel (section nav + content) in the spirit of Gmail / Outlook
// settings. Sections: General (signature + vacation), Filters & rules, and —
// for admins — the tenant Organization footer. General/Organization save via
// the footer button; Filters persist themselves.
import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { Building2, PenLine, SlidersHorizontal, Users, X } from "lucide-react";

import { strings } from "../i18n";
import { Button, Spinner } from "../ds";
import { useJmapClient } from "../jmap";
import { RichTextEditor } from "../mail/components/RichTextEditor";
import { FiltersSection } from "./FiltersSection";
import { SharingSection } from "./SharingSection";
import styles from "./SettingsModal.module.css";

interface SettingsModalProps {
  isAdmin: boolean;
  onClose: () => void;
}

type Tab = "general" | "filters" | "sharing" | "org";

export function SettingsModal({ isAdmin, onClose }: SettingsModalProps) {
  const client = useJmapClient();
  const [tab, setTab] = useState<Tab>("general");
  const [loaded, setLoaded] = useState(false);
  const [signature, setSignature] = useState("");
  const [orgFooter, setOrgFooter] = useState("");
  const [oooEnabled, setOooEnabled] = useState(false);
  const [oooSubject, setOooSubject] = useState("");
  const [oooMessage, setOooMessage] = useState("");
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
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      await client.setSignature(signature);
      await client.setOutOfOffice(oooEnabled, oooSubject, oooMessage);
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
    <div className={styles.overlay} onMouseDown={onClose}>
      <div
        className={styles.modal}
        role="dialog"
        aria-modal="true"
        aria-label={strings.settingsTitle}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className={styles.head}>
          <span className={styles.headIcon}>
            <SlidersHorizontal size={17} />
          </span>
          <h2>{strings.settingsTitle}</h2>
          <button
            type="button"
            className={styles.iconBtn}
            onClick={onClose}
            aria-label={strings.userClose}
          >
            <X size={18} />
          </button>
        </div>

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
                      <label className={styles.oooRow}>
                        <span className={styles.oooRowText}>
                          <span className={styles.oooRowTitle}>
                            {strings.settingsOooToggle}
                          </span>
                          <span className={styles.oooRowHint}>
                            {strings.settingsOutOfOfficeHint}
                          </span>
                        </span>
                        <span className={styles.toggle}>
                          <input
                            type="checkbox"
                            checked={oooEnabled}
                            onChange={(e) => setOooEnabled(e.target.checked)}
                          />
                          <span className={styles.track} />
                        </span>
                      </label>
                      {oooEnabled && (
                        <>
                          <input
                            className={styles.input}
                            value={oooSubject}
                            onChange={(e) => setOooSubject(e.target.value)}
                            placeholder={strings.settingsOooSubjectPlaceholder}
                          />
                          <textarea
                            className={styles.textarea}
                            rows={4}
                            value={oooMessage}
                            onChange={(e) => setOooMessage(e.target.value)}
                            placeholder={strings.settingsOooMessagePlaceholder}
                          />
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

        <div className={styles.foot}>
          <span className={styles.footMsg}>
            {note !== null && <span className={styles.footOk}>{note}</span>}
            {error !== null && (
              <span className={styles.footErr} role="alert">
                {error}
              </span>
            )}
          </span>
          <button type="button" className={styles.textBtn} onClick={onClose}>
            {strings.userClose}
          </button>
          <Button onClick={() => void save()} disabled={busy || !loaded}>
            {busy ? <Spinner size={16} /> : strings.settingsSave}
          </Button>
        </div>
      </div>
    </div>
  );
}
