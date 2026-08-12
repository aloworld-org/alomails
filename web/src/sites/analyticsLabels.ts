// Naming the stored analytics buckets in the reader's language. The server
// deliberately stores stable, unit-suffixed tokens ("1-3m", "phone", "NL", "")
// and leaves the words to the interface — so this is the only place that turns
// one into the other, and it is pure so a test can pin every bucket.
import { strings } from "../i18n";

/** How long a page view stayed readable. Keys are the stored labels of
 *  `ReadTimeBucket`; anything else is a bucket this build does not know. */
const READ_TIME: Record<string, () => string> = {
  "0-10s": () => strings.sitesAnalyticsReadUnder10s,
  "10-30s": () => strings.sitesAnalyticsRead10to30s,
  "30-60s": () => strings.sitesAnalyticsRead30to60s,
  "1-3m": () => strings.sitesAnalyticsRead1to3m,
  "3-10m": () => strings.sitesAnalyticsRead3to10m,
  "10m+": () => strings.sitesAnalyticsReadOver10m,
};

/** The five device classes the public service derives and then forgets. */
const DEVICES: Record<string, () => string> = {
  phone: () => strings.sitesAnalyticsDevicePhone,
  tablet: () => strings.sitesAnalyticsDeviceTablet,
  desktop: () => strings.sitesAnalyticsDeviceDesktop,
  bot: () => strings.sitesAnalyticsDeviceBot,
  unknown: () => strings.sitesAnalyticsDeviceUnknown,
};

/** The overflow bucket of the outbound dimension. It can never collide with a
 *  real destination, because a stored domain always contains a dot. */
const OUTBOUND_OVERFLOW = "other";

/** Region names in the reader's locale, built once. Undefined locales means
 *  "whatever this browser is set to", as everywhere else in this module. */
let regions: Intl.DisplayNames | null | undefined;

function regionNames(): Intl.DisplayNames | null {
  if (regions === undefined) {
    try {
      regions = new Intl.DisplayNames(undefined, { type: "region" });
    } catch {
      regions = null;
    }
  }
  return regions;
}

/** A two-letter code as a country name, falling back to the code itself. A
 *  structurally valid but unassigned code makes `Intl` throw, and an owner is
 *  better served by "ZZ" than by a screen that does not render. */
export function countryLabel(code: string): string {
  if (code === "") return strings.sitesAnalyticsNotReported;
  if (!/^[A-Z]{2}$/.test(code)) return code;
  try {
    return regionNames()?.of(code) ?? code;
  } catch {
    return code;
  }
}

/** A reading-time bucket as a phrase. An unknown token is shown verbatim: a
 *  server that grew a seventh bucket should not disappear from the histogram. */
export function readTimeLabel(bucket: string): string {
  return READ_TIME[bucket]?.() ?? bucket;
}

/** A device class as a word. */
export function deviceLabel(bucket: string): string {
  return DEVICES[bucket]?.() ?? bucket;
}

/** A campaign label. Empty means the visit arrived on a link with no
 *  `utm_campaign` — most visits, on most sites. */
export function campaignLabel(label: string): string {
  return label === "" ? strings.sitesAnalyticsNoCampaign : label;
}

/** An outbound destination. Only the literal overflow bucket is renamed. */
export function outboundLabel(domain: string): string {
  if (domain === OUTBOUND_OVERFLOW) return strings.sitesAnalyticsOutboundOther;
  return domain === "" ? strings.sitesAnalyticsNotReported : domain;
}

/** A referrer domain. Empty means the visitor typed the address or followed a
 *  link the browser did not name. */
export function referrerLabel(domain: string): string {
  return domain === "" ? strings.sitesAnalyticsDirect : domain;
}

/** A page path, shown as stored. An empty path cannot occur — the public
 *  service normalises "/" — but naming it beats rendering a blank row. */
export function pathLabel(path: string): string {
  return path === "" ? strings.sitesAnalyticsNotReported : path;
}
