// The gallery: the ready-made questions a reader pins to a board (ADR 0037,
// wave BI1.06).
//
// The server sends each entry as a **key and a question**, never a caption —
// English from a server is English for everybody — so the words on this screen
// come from the catalog in `i18n`, and the caption that is stored with the tile
// is the one the reader was actually looking at. From that moment it is their
// own text, renameable, and never re-translated behind their back.
//
// Nothing here decides whether a chart is askable: the spec is handed straight
// back to the server, which validates it through the same write gate a builder
// or a model would meet. And nothing here computes a figure — a pinned tile
// asks for its own numbers, like every other tile on the board.
//
// The frame is `ds/Modal` since D2.08. This dialog handled Escape on the panel
// alone, so the key only worked once focus was already inside it, and nothing
// stopped Tab walking out onto the board behind — which is the part of a dialog
// that is invisible until somebody tries to use it from a keyboard.
import { useState } from "react";
import { Sparkles, X } from "lucide-react";

import { Button, IconButton, Modal, Spinner } from "../ds";
import { strings } from "../i18n";
import { ErrorBanner } from "./parts";
import type { GalleryEntry, GalleryModule } from "./types";
import { useGallery } from "./useInsights";
import styles from "./InsightsModule.module.css";

/** The words for one prebuilt question, against the key the server sends. The
 *  mapping is code and the words are the catalog's, so a language that has not
 *  translated one entry still shows the rest in its own words.
 *
 *  A key this build has no words for — a newer server offering a question we do
 *  not know yet — is shown under its own key rather than hidden: an entry a
 *  reader can see and pin is worth more than one silently dropped. */
function words(key: string): { title: string; body: string } {
  switch (key) {
    case "revenue_by_month":
      return {
        title: strings.insightsGalleryRevenueByMonth,
        body: strings.insightsGalleryRevenueByMonthBody,
      };
    case "outstanding":
      return {
        title: strings.insightsGalleryOutstanding,
        body: strings.insightsGalleryOutstandingBody,
      };
    case "overdue_aging":
      return {
        title: strings.insightsGalleryOverdueAging,
        body: strings.insightsGalleryOverdueAgingBody,
      };
    case "vat_by_quarter":
      return {
        title: strings.insightsGalleryVatByQuarter,
        body: strings.insightsGalleryVatByQuarterBody,
      };
    case "top_customers":
      return {
        title: strings.insightsGalleryTopCustomers,
        body: strings.insightsGalleryTopCustomersBody,
      };
    case "payments_by_month":
      return {
        title: strings.insightsGalleryPaymentsByMonth,
        body: strings.insightsGalleryPaymentsByMonthBody,
      };
    case "pipeline_by_stage":
      return {
        title: strings.insightsGalleryPipelineByStage,
        body: strings.insightsGalleryPipelineByStageBody,
      };
    case "won_this_month":
      return {
        title: strings.insightsGalleryWonThisMonth,
        body: strings.insightsGalleryWonThisMonthBody,
      };
    case "win_rate_by_quarter":
      return {
        title: strings.insightsGalleryWinRateByQuarter,
        body: strings.insightsGalleryWinRateByQuarterBody,
      };
    case "won_by_month":
      return {
        title: strings.insightsGalleryWonByMonth,
        body: strings.insightsGalleryWonByMonthBody,
      };
    default:
      return { title: key, body: "" };
  }
}

/** The modules, in the order the gallery groups them. */
const MODULES: GalleryModule[] = ["billing", "crm"];

function moduleLabel(module: GalleryModule): string {
  return module === "billing" ? strings.moduleBilling : strings.moduleCrm;
}

/** Picks a question and pins it. Closing without picking sends nothing. */
export function GalleryDialog({
  busy,
  error,
  onPick,
  onClose,
}: {
  busy: boolean;
  error: string | null;
  onPick: (entry: GalleryEntry, title: string) => void;
  onClose: () => void;
}) {
  const { gallery, loading, error: loadError } = useGallery(true);
  const [picked, setPicked] = useState<string | null>(null);
  const banner = error ?? loadError;

  function pick(entry: GalleryEntry) {
    if (busy) return;
    setPicked(entry.key);
    onPick(entry, words(entry.key).title);
  }

  return (
    <Modal
      title={strings.insightsGalleryTitle}
      onClose={onClose}
      icon={<Sparkles size={19} />}
      actions={
        <IconButton
          label={strings.insightsGalleryClose}
          icon={<X size={18} />}
          onClick={onClose}
        />
      }
      footer={
        <>
          <div className="flex-1" />
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {strings.dialogCancel}
          </Button>
        </>
      }
    >
      {/* `ds/Modal`'s header is the name and the controls, so what this dialog
          is for reads as the first line of the body rather than as a second
          heading — the same shape `crm/DialogFrame` settled on. */}
      <p className="m-0 text-sm text-tertiary">
        {strings.insightsGallerySubtitle}
      </p>
      {banner !== null && <ErrorBanner message={banner} />}
      {loading && gallery.entries.length === 0 && <Spinner size={18} />}
      {MODULES.map((module) => {
        const entries = gallery.entries.filter(
          (entry) => entry.module === module,
        );
        if (entries.length === 0) return null;
        return (
          <section key={module} className={styles.gallerySection}>
            <h3 className={styles.galleryGroup}>{moduleLabel(module)}</h3>
            <ul className={styles.galleryList}>
              {entries.map((entry) => (
                <li key={entry.key}>
                  <button
                    type="button"
                    className={styles.galleryEntry}
                    disabled={busy}
                    onClick={() => pick(entry)}
                  >
                    <span className={styles.galleryEntryTitle}>
                      {words(entry.key).title}
                    </span>
                    <span className={styles.galleryEntryBody}>
                      {words(entry.key).body}
                    </span>
                    {busy && picked === entry.key && <Spinner size={14} />}
                  </button>
                </li>
              ))}
            </ul>
          </section>
        );
      })}
    </Modal>
  );
}
