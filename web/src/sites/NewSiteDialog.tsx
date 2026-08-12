// Website onboarding has two complete, visible paths: describe the business
// and let alo prepare a private draft, or start manually from a shipped
// template — a finished three-page site the tenant then edits. AI is
// acceleration, never a gate: an unconfigured tenant moves directly to the
// manual path, which creates a real site with its pages end to end.
import { useEffect, useRef, useState } from "react";
import { Globe, LayoutTemplate, Sparkles } from "lucide-react";

import { strings } from "../i18n";
import { SitesError, sitesMessage, useSitesApi } from "./api";
import { DialogFrame, Field } from "./parts";
import { TemplateGallery } from "./TemplateGallery";
import type { Site, SitePage } from "./types";
import styles from "./SitesModule.module.css";

/** How long the address field stays still before the availability question is
 *  asked — long enough to skip mid-word states, short enough to feel live. */
export const SUBDOMAIN_CHECK_DELAY_MS = 350;

type Mode = "generate" | "template";
type Check =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "available"; subdomain: string }
  | { kind: "taken"; subdomain: string }
  | { kind: "invalid"; message: string };

/** The address suggestion is deliberately syntax-only. The store remains the
 *  one authority on validity and reserved words. */
export function siteAddressSuggestion(name: string): string {
  return name
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40)
    .replace(/-+$/g, "");
}

/** Accept the label owners expect as well as the complete address they often
 *  paste from a browser. Matching the configured suffix is presentation
 *  normalization, not a second copy of the server's validation rules. */
export function normalizeSiteAddress(value: string, domain: string | null): string {
  const host = value
    .trim()
    .replace(/^[a-z][a-z0-9+.-]*:\/\//i, "")
    .split(/[/?#]/, 1)[0]!
    .replace(/\.$/, "")
    .toLowerCase();
  if (domain === null) return host;
  const suffix = `.${domain.toLowerCase()}`;
  return host.endsWith(suffix) ? host.slice(0, -suffix.length) : host;
}

function AddressStatus({
  check,
  subdomain,
  domain,
}: {
  check: Check;
  subdomain: string;
  domain: string | null;
}) {
  if (subdomain === "") return null;
  const address = domain === null ? subdomain : `${subdomain}.${domain}`;
  const text =
    check.kind === "checking"
      ? strings.sitesSubdomainChecking
      : check.kind === "available"
        ? strings.sitesAddressAvailable
        : check.kind === "taken"
          ? strings.sitesAddressTaken
          : check.kind === "invalid"
            ? check.message
            : strings.sitesAddressNotChecked;
  const tone =
    check.kind === "available"
      ? styles.checkOk
      : check.kind === "checking" || check.kind === "idle"
        ? styles.checkPending
        : styles.checkBad;
  return (
    <p className={`${styles.addressStatus} ${tone}`} role="status">
      <Globe aria-hidden="true" />
      <span className={styles.addressValue}>{address}</span>
      <span>{text}</span>
    </p>
  );
}

export function NewSiteDialog({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (site: Site, page: SitePage) => void;
}) {
  const api = useSitesApi();
  const [mode, setMode] = useState<Mode>("generate");
  const [description, setDescription] = useState("");
  const [name, setName] = useState("");
  const [subdomain, setSubdomain] = useState("");
  const [check, setCheck] = useState<Check>({ kind: "idle" });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [domain, setDomain] = useState<string | null>(null);
  const [template, setTemplate] = useState<string | null>(null);
  const checkSeq = useRef(0);
  const addressEdited = useRef(false);

  useEffect(() => {
    let cancelled = false;
    api.config().then(
      (c) => {
        if (!cancelled && typeof c.domain === "string" && c.domain.trim() !== "") {
          const nextDomain = c.domain.trim().toLowerCase();
          setDomain(nextDomain);
          setSubdomain((current) => normalizeSiteAddress(current, nextDomain));
        }
      },
      () => undefined,
    );
    return () => {
      cancelled = true;
    };
  }, [api]);

  useEffect(() => {
    if (mode !== "template") return;
    const value = subdomain.trim().toLowerCase();
    const mine = ++checkSeq.current;
    if (value === "") {
      setCheck({ kind: "idle" });
      return;
    }
    setCheck({ kind: "checking" });
    const timer = setTimeout(() => {
      api.checkSubdomain(value).then(
        (answer) => {
          if (checkSeq.current !== mine) return;
          setCheck(
            answer.available
              ? { kind: "available", subdomain: answer.subdomain }
              : { kind: "taken", subdomain: answer.subdomain },
          );
        },
        (err: unknown) => {
          if (checkSeq.current !== mine) return;
          setCheck({ kind: "invalid", message: sitesMessage(err, strings.sitesCheckFailed) });
        },
      );
    }, SUBDOMAIN_CHECK_DELAY_MS);
    return () => clearTimeout(timer);
  }, [subdomain, api, mode]);

  function chooseMode(next: Mode) {
    setMode(next);
    setError(null);
    setNotice(null);
  }

  async function generate() {
    setBusy(true);
    setError(null);
    try {
      const draft = await api.generateSite(description.trim());
      const home = draft.pages.find((page) => page.home) ?? draft.pages[0];
      if (home === undefined) throw new SitesError(422, strings.sitesGenerationEmpty);
      onCreated(draft.site, home);
    } catch (err) {
      if (err instanceof SitesError && err.reason === "unconfigured") {
        setMode("template");
        setNotice(strings.sitesGenerationUnavailable);
      } else {
        setError(sitesMessage(err, strings.sitesGenerationFailed));
      }
      setBusy(false);
    }
  }

  /** The manual path: one shipped template instantiated in a single server
   *  transaction, or — with no template chosen — a blank site whose Home page
   *  is created straight away, so both endings land in the editor (S1.30c). */
  async function createFromTemplate() {
    setBusy(true);
    setError(null);
    const draft = {
      name: name.trim(),
      subdomain: normalizeSiteAddress(subdomain, domain),
    };
    try {
      if (template !== null) {
        const created = await api.createSiteFromTemplate(template, draft);
        const home = created.pages.find((page) => page.home) ?? created.pages[0];
        if (home === undefined) throw new SitesError(422, strings.sitesGenerationEmpty);
        onCreated(created.site, home);
        return;
      }
      const site = await api.createSite(draft);
      const home = await api.createPage(site.id, {
        title: strings.sitesHomePageTitle,
        slug: "",
        home: true,
      });
      onCreated(site, home);
    } catch (err) {
      setError(sitesMessage(err, strings.sitesSaveFailed));
      setBusy(false);
    }
  }

  const canSubmit =
    mode === "generate"
      ? description.trim() !== ""
      : name.trim() !== "" && subdomain.trim() !== "";
  const missingTemplateValue =
    mode !== "template" || canSubmit
      ? null
      : name.trim() === ""
        ? strings.sitesNameRequired
        : strings.sitesAddressRequired;

  return (
    <DialogFrame
      Icon={mode === "generate" ? Sparkles : LayoutTemplate}
      // The gallery needs the room its cards and preview ask for; the
      // description path stays the narrow form it always was.
      wide={mode === "template"}
      title={strings.sitesNewSiteTitle}
      subtitle={strings.sitesNewSiteSubtitle}
      error={error}
      busy={busy}
      canSubmit={canSubmit}
      submitLabel={
        busy
          ? mode === "generate"
            ? strings.sitesGenerating
            : strings.sitesCreatingSite
          : mode === "generate"
            ? strings.sitesGenerateSite
            : strings.sitesCreateSite
      }
      onClose={onClose}
      onSubmit={() => void (mode === "generate" ? generate() : createFromTemplate())}
    >
      <div className={styles.onboardingChoices} aria-label={strings.sitesStartingPoint}>
        <button
          type="button"
          className={mode === "generate" ? `${styles.onboardingChoice} ${styles.onboardingChoiceActive}` : styles.onboardingChoice}
          aria-pressed={mode === "generate"}
          onClick={() => chooseMode("generate")}
        >
          <Sparkles aria-hidden="true" />
          <span>{strings.sitesGenerateChoice}</span>
        </button>
        <button
          type="button"
          className={mode === "template" ? `${styles.onboardingChoice} ${styles.onboardingChoiceActive}` : styles.onboardingChoice}
          aria-pressed={mode === "template"}
          onClick={() => chooseMode("template")}
        >
          <LayoutTemplate aria-hidden="true" />
          <span>{strings.sitesTemplateChoice}</span>
        </button>
      </div>

      {notice !== null && <p className={styles.onboardingNotice} role="status">{notice}</p>}

      {mode === "generate" ? (
        <Field label={strings.sitesBusinessDescription} hint={strings.sitesBusinessDescriptionHint}>
          <textarea
            className={`${styles.input} ${styles.onboardingDescription}`}
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            placeholder={strings.sitesBusinessDescriptionPlaceholder}
            autoFocus
          />
        </Field>
      ) : (
        <>
          <Field label={strings.sitesFieldName}>
            <input
              className={styles.input}
              value={name}
              onChange={(event) => {
                const nextName = event.target.value;
                setName(nextName);
                if (!addressEdited.current) setSubdomain(siteAddressSuggestion(nextName));
              }}
              autoFocus
            />
          </Field>
          <Field label={strings.sitesFieldSubdomain} hint={strings.sitesSubdomainHint}>
            <input
              className={styles.input}
              value={subdomain}
              onChange={(event) => {
                addressEdited.current = true;
                setSubdomain(normalizeSiteAddress(event.target.value, domain));
              }}
              autoCapitalize="none"
              autoCorrect="off"
              spellCheck={false}
            />
          </Field>
          <AddressStatus check={check} subdomain={subdomain.trim()} domain={domain} />
          {missingTemplateValue !== null && (
            <p className={styles.submitRequirement} role="status">{missingTemplateValue}</p>
          )}
          <TemplateGallery
            selected={template}
            onSelect={(choice) =>
              setTemplate(choice.kind === "blank" ? null : choice.template.id)
            }
          />
        </>
      )}
    </DialogFrame>
  );
}
