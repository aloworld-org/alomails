// Website onboarding has two complete, visible paths: describe the business
// and let alo prepare a private draft, or start manually from a shipped
// template — a finished three-page site the tenant then edits. AI is
// acceleration, never a gate: an unconfigured tenant moves directly to the
// manual path, which creates a real site with its pages end to end.
import { useEffect, useRef, useState } from "react";
import { CheckCircle2, Globe, LayoutTemplate, Sparkles } from "lucide-react";

import { strings } from "../i18n";
import { SitesError, sitesMessage, useSitesApi } from "./api";
import { DialogFrame, Field } from "./parts";
import { TemplateGallery } from "./TemplateGallery";
import type { Site, SitePage } from "./types";

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
export function normalizeSiteAddress(
  value: string,
  domain: string | null,
): string {
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
      ? "border-success bg-success-tint text-success"
      : check.kind === "checking" || check.kind === "idle"
        ? "border-default bg-raised text-secondary"
        : "border-danger bg-danger-tint text-danger";
  return (
    <p
      className={`flex flex-wrap items-center gap-2 rounded-xl border px-3 py-2.5 text-sm ${tone}`}
      role="status"
    >
      <Globe className="size-4 shrink-0" aria-hidden="true" />
      <span className="font-mono font-medium text-primary">{address}</span>
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
        if (
          !cancelled &&
          typeof c.domain === "string" &&
          c.domain.trim() !== ""
        ) {
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
          setCheck({
            kind: "invalid",
            message: sitesMessage(err, strings.sitesCheckFailed),
          });
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
      if (home === undefined)
        throw new SitesError(422, strings.sitesGenerationEmpty);
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
        const home =
          created.pages.find((page) => page.home) ?? created.pages[0];
        if (home === undefined)
          throw new SitesError(422, strings.sitesGenerationEmpty);
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
      : name.trim() !== "" &&
        subdomain.trim() !== "" &&
        check.kind === "available" &&
        check.subdomain === normalizeSiteAddress(subdomain, domain);
  const missingTemplateValue =
    mode !== "template"
      ? null
      : name.trim() === ""
        ? strings.sitesNameRequired
        : subdomain.trim() === ""
          ? strings.sitesAddressRequired
          : null;

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
      onSubmit={() =>
        void (mode === "generate" ? generate() : createFromTemplate())
      }
    >
      <div>
        <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-tertiary">
          {strings.sitesStartingPoint}
        </p>
        <div
          className="grid grid-cols-2 gap-2"
          aria-label={strings.sitesStartingPoint}
        >
          <button
            type="button"
            aria-label={strings.sitesGenerateChoice}
            className={`relative flex min-w-0 items-start gap-3 rounded-xl border p-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent sm:p-4 ${mode === "generate" ? "border-accent bg-accent-soft" : "border-default bg-surface hover:border-strong hover:bg-raised"}`}
            aria-pressed={mode === "generate"}
            onClick={() => chooseMode("generate")}
          >
            <span
              className={`grid size-9 shrink-0 place-items-center rounded-lg ${mode === "generate" ? "bg-accent text-on-accent" : "bg-accent-soft text-accent"}`}
            >
              <Sparkles className="size-4" aria-hidden="true" />
            </span>
            <span className="min-w-0">
              <span className="block text-sm font-semibold text-primary">
                {strings.sitesGenerateChoice}
              </span>
              <span className="mt-0.5 hidden text-xs leading-relaxed text-secondary sm:block">
                {strings.sitesGenerateChoiceDescription}
              </span>
            </span>
            {mode === "generate" && (
              <CheckCircle2
                className="absolute right-2 top-2 size-4 text-accent"
                aria-hidden="true"
              />
            )}
          </button>
          <button
            type="button"
            aria-label={strings.sitesTemplateChoice}
            className={`relative flex min-w-0 items-start gap-3 rounded-xl border p-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent sm:p-4 ${mode === "template" ? "border-accent bg-accent-soft" : "border-default bg-surface hover:border-strong hover:bg-raised"}`}
            aria-pressed={mode === "template"}
            onClick={() => chooseMode("template")}
          >
            <span
              className={`grid size-9 shrink-0 place-items-center rounded-lg ${mode === "template" ? "bg-accent text-on-accent" : "bg-accent-soft text-accent"}`}
            >
              <LayoutTemplate className="size-4" aria-hidden="true" />
            </span>
            <span className="min-w-0">
              <span className="block text-sm font-semibold text-primary">
                {strings.sitesTemplateChoice}
              </span>
              <span className="mt-0.5 hidden text-xs leading-relaxed text-secondary sm:block">
                {strings.sitesTemplateChoiceDescription}
              </span>
            </span>
            {mode === "template" && (
              <CheckCircle2
                className="absolute right-2 top-2 size-4 text-accent"
                aria-hidden="true"
              />
            )}
          </button>
        </div>
      </div>

      {notice !== null && (
        <p
          className="rounded-xl border border-default bg-raised px-4 py-3 text-sm text-secondary"
          role="status"
        >
          {notice}
        </p>
      )}

      {mode === "generate" ? (
        <Field
          label={strings.sitesBusinessDescription}
          hint={strings.sitesBusinessDescriptionHint}
        >
          <textarea
            className="min-h-40 w-full resize-y rounded-xl border border-default bg-surface px-4 py-3 text-base text-primary outline-none transition-shadow placeholder:text-tertiary focus:border-accent focus:ring-2 focus:ring-accent-soft"
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            placeholder={strings.sitesBusinessDescriptionPlaceholder}
            autoFocus
          />
        </Field>
      ) : (
        <>
          <div className="grid gap-4 rounded-2xl bg-raised p-4 sm:grid-cols-2">
            <Field label={strings.sitesFieldName}>
              <input
                className="h-11 w-full rounded-xl border border-default bg-surface px-3.5 text-base text-primary outline-none transition-shadow focus:border-accent focus:ring-2 focus:ring-accent-soft"
                value={name}
                onChange={(event) => {
                  const nextName = event.target.value;
                  setName(nextName);
                  if (!addressEdited.current)
                    setSubdomain(siteAddressSuggestion(nextName));
                }}
                autoFocus
              />
            </Field>
            <Field
              label={strings.sitesFieldSubdomain}
              hint={strings.sitesSubdomainHint}
            >
              <input
                className="h-11 w-full rounded-xl border border-default bg-surface px-3.5 font-mono text-base text-primary outline-none transition-shadow focus:border-accent focus:ring-2 focus:ring-accent-soft"
                value={subdomain}
                onChange={(event) => {
                  addressEdited.current = true;
                  setSubdomain(
                    normalizeSiteAddress(event.target.value, domain),
                  );
                }}
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
              />
            </Field>
            <div className="sm:col-span-2">
              <AddressStatus
                check={check}
                subdomain={subdomain.trim()}
                domain={domain}
              />
              {missingTemplateValue !== null && (
                <p className="mt-2 text-sm text-secondary" role="status">
                  {missingTemplateValue}
                </p>
              )}
            </div>
          </div>
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
