// Billing's own layout, as Tailwind recipes. Keeping repeated patterns here
// gives every ledger, form and document the same rhythm without a CSS module.
//
// # What does NOT belong here
//
// The primitives. Until D2.06b this file also declared `input`, `select`,
// `chip`, `badge`, `table`, `toolbar` and `toggle` — a seventh through
// thirteenth copy of controls `ds/` owns, read by nineteen `.tsx` files. They
// survived the whole D1/D2 wave for one reason: `ds/redefined.ts` lists
// *stylesheets*, and `primitives.test.ts` reads `.module.css`. A recipe in a
// `.ts` file is invisible to both. So the rule this file has to keep itself is
// the one the build cannot: **a control belongs to `ds/`, and what is left
// here is arrangement** — where things sit, how wide they are, what a table row
// means. If a screen needs a control this module does not have, widen the `ds/`
// component; do not start a fourteenth copy in here.
//
// The one deliberate exception is `textarea`, and it is temporary: the design
// system still has no multi-line control, so the box below is `ds/Input`'s
// written out for a `<textarea>`. It goes when that component exists.
const styles = {
  page: "mx-auto flex min-h-0 w-full max-w-[112rem] flex-1 flex-col gap-5 overflow-y-auto px-8 pb-0 pt-6 max-[52rem]:px-4 max-[52rem]:pb-0 max-[52rem]:pt-4",
  /** What billing adds to `ds/Toolbar` above a list: a minimum height so a bar
   *  with only a heading in it still reads as a bar, and controls that stretch
   *  rather than centre once the row has wrapped. */
  listBar:
    "min-h-14 shrink-0 rounded-2xl border border-default bg-surface px-4 py-3 shadow-sm max-[52rem]:items-stretch",
  searchWrap:
    "relative flex max-w-[380px] flex-1 items-center [&>svg]:pointer-events-none [&>svg]:absolute [&>svg]:right-3 [&>svg]:size-4 [&>svg]:text-tertiary max-[52rem]:max-w-none max-[52rem]:basis-full",
  /** A filter's name, sitting beside the `ds/Select` it names. */
  filterLabel:
    "inline-flex items-center gap-2 whitespace-nowrap text-sm text-secondary",
  /** What a list's `ds/Table` adds: it is the page's scrolling region, so it
   *  takes the space the toolbar above it leaves. */
  listTable: "min-h-0 flex-1",
  /** A column of a line grid that holds a short figure — a quantity, a rate.
   *  The width is on the column rather than on the control inside it: the
   *  control is `ds/Input`, which fills what holds it. */
  narrowCol: "w-[8ch]",
  numeric: "text-right tabular-nums",
  mono: "font-mono text-sm",
  rowName:
    "rounded-md text-left text-sm font-medium text-primary hover:text-accent",
  archivedRow: "opacity-60",
  rowActions: "whitespace-nowrap text-right",
  linkAction:
    "inline-flex min-h-9 items-center gap-1.5 rounded-lg bg-transparent !px-3 !py-2 text-sm font-medium !text-secondary !no-underline transition-colors hover:bg-raised hover:!text-accent hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent disabled:cursor-not-allowed disabled:bg-transparent disabled:!text-tertiary disabled:opacity-55 disabled:hover:bg-transparent disabled:hover:!text-tertiary",
  empty:
    "flex min-h-72 flex-1 flex-col items-center justify-center gap-2 rounded-xl border border-subtle bg-surface p-8 text-center",
  customerEmptyLayout: "flex min-h-0 flex-1 overflow-auto pb-2",
  customerEmptyCard:
    "flex min-h-96 flex-1 rounded-xl border border-subtle bg-surface shadow-sm [&>div]:w-full [&>div]:border-0",
  emptyArt:
    "relative inline-flex size-12 items-center justify-center rounded-xl bg-[var(--accent-soft)] text-accent",
  emptyWithAction: "[&>span]:mb-2",
  emptyTitle: "m-0 text-xl font-semibold text-primary",
  emptyBody: "mb-2 max-w-[50ch] text-sm leading-relaxed text-secondary",
  noMatches:
    "min-h-48 rounded-xl border border-dashed border-default bg-surface px-6 py-8 text-center text-sm text-tertiary",
  error:
    "m-0 rounded-lg border border-danger bg-[var(--danger-tint)] p-3 text-sm text-primary",
  row: "flex gap-4 [&>*]:min-w-0 [&>*]:flex-1",
  hint: "text-xs leading-relaxed text-tertiary",
  fieldError: "text-xs leading-relaxed text-danger",
  /** `ds/Input`'s box, written out for a `<textarea>` — the same border,
   *  radius, height rhythm and focus ring — because the design system has no
   *  multi-line control yet. The moment it has one, this key and its three
   *  callers go. Do not reach for it as a general text-box recipe: a
   *  single-line field is `ds/Input`. */
  textarea:
    "w-full min-h-16 resize-y rounded-md border border-default bg-surface px-3 py-2 font-[inherit] text-base text-primary placeholder:text-tertiary transition-colors duration-[var(--duration-fast)] ease-standard focus:border-accent focus:outline-none focus-visible:border-accent focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-1",
  templatePicker: "mb-6 flex flex-col gap-3",
  templatePickerTitle: "m-0 text-sm font-semibold text-primary",
  templatePickerHint: "mb-0 mt-1 text-xs leading-relaxed text-tertiary",
  templateGrid:
    "grid grid-cols-4 gap-4 max-[46rem]:grid-cols-2 max-[28rem]:grid-cols-1",
  templateCard:
    "group relative flex min-h-60 flex-col items-start rounded-2xl border border-default bg-surface !p-6 text-left shadow-sm transition-[border-color,background-color] duration-150 hover:border-accent/50 focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-accent/15",
  templateCardActive:
    "!border-accent bg-surface ring-1 ring-accent/15 [&_.template-name]:text-accent",
  templateCardPreview:
    "block h-32 w-full overflow-hidden rounded-xl bg-raised p-3",
  templateCardFooter:
    "mt-5 flex w-full items-center justify-between gap-3 border-t border-subtle pt-4",
  templateCardName:
    "template-name block min-w-0 text-sm font-semibold leading-snug text-primary",
  templateCardCheck:
    "inline-flex size-6 shrink-0 items-center justify-center rounded-full border border-default bg-surface text-transparent",
  templateCardCheckActive: "border-accent bg-accent text-on-accent",
  templateCardDescription: "sr-only",
  templateItems:
    "rounded-2xl border border-default bg-surface p-5 text-sm text-secondary shadow-sm max-[34rem]:p-4",
  templateItemsHead:
    "flex items-start justify-between gap-4 max-[34rem]:flex-col",
  templateItemsTitle: "m-0 text-sm font-semibold text-primary",
  templateItemsHint:
    "mb-0 mt-1 max-w-2xl text-xs leading-relaxed text-secondary",
  templateItemsCount:
    "inline-flex min-h-8 shrink-0 items-center rounded-full bg-accent-soft px-3 text-xs font-semibold text-accent ring-1 ring-accent/15",
  templateAddItems:
    "inline-flex h-10 shrink-0 items-center justify-center gap-2 rounded-xl bg-accent !px-4 text-sm font-medium text-on-accent shadow-sm transition-colors duration-150 hover:bg-[#D96247] focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-accent/15",
  templateItemsList: "mt-3 grid grid-cols-2 gap-2 max-[38rem]:grid-cols-1",
  templateItem:
    "flex min-h-16 items-center gap-3 rounded-xl border border-subtle bg-raised px-4 py-3 text-sm text-primary",
  templateItemCheck:
    "inline-flex size-7 shrink-0 items-center justify-center rounded-full bg-accent-soft text-accent",
  templateItemMeta:
    "mt-0.5 flex items-center gap-1.5 text-xs font-normal text-secondary",
  templateItemRemove:
    "inline-flex size-9 shrink-0 items-center justify-center rounded-xl text-tertiary transition-colors hover:bg-[var(--danger-tint)] hover:text-danger focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-danger/30",
  templateProductPicker:
    "mt-4 rounded-xl border border-accent/20 bg-surface p-3 shadow-sm",
  templateProductSearch:
    "relative block [&>svg]:pointer-events-none [&>svg]:absolute [&>svg]:left-4 [&>svg]:top-1/2 [&>svg]:z-10 [&>svg]:-translate-y-1/2 [&>svg]:text-tertiary [&_input]:!pl-11",
  templateProductList:
    "mt-3 grid max-h-52 grid-cols-2 gap-2 overflow-y-auto pr-1 max-[38rem]:grid-cols-1",
  templateProductOption:
    "flex min-h-11 items-center gap-3 rounded-lg border border-subtle bg-surface !px-4 !py-2.5 text-left text-sm transition-[border-color,background-color] hover:border-accent/50 hover:bg-accent-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30",
  templateProductEmpty:
    "m-0 mt-3 rounded-lg bg-accent-soft px-4 py-3 text-center text-sm text-secondary",
  chips: "inline-flex flex-wrap items-center gap-2",
  overdueRow: "[&_td]:bg-[var(--danger-tint)]",
  notice:
    "mx-6 mb-0 mt-5 rounded-xl border border-default bg-raised px-4 py-3 text-sm leading-relaxed text-secondary max-[52rem]:mx-4",
  loading: "flex flex-1 items-center justify-center p-10",
  dataLoading:
    "flex min-h-48 flex-1 flex-col items-center justify-center gap-3 rounded-xl border border-subtle bg-surface text-sm text-secondary",
  editor:
    "min-h-full shrink-0 overflow-hidden rounded-t-2xl border border-b-0 border-default bg-surface shadow-sm",
  editorHead:
    "flex min-h-20 flex-wrap items-center gap-3 border-b border-subtle bg-surface px-6 py-4 max-[52rem]:px-4",
  editorTitle: "m-0 text-xl font-semibold tracking-tight text-primary",
  saveState: "ml-auto whitespace-nowrap text-xs text-tertiary",
  editorBody: "flex flex-col gap-6 px-6 pb-8 pt-6 max-[52rem]:px-4",
  quoteHero:
    "relative flex min-h-44 items-center justify-between gap-8 overflow-hidden rounded-2xl border border-default bg-[var(--quote-background,var(--bg-surface))] px-7 py-6 text-[var(--quote-text,var(--text-primary))] shadow-sm before:absolute before:bottom-0 before:left-0 before:top-0 before:w-1 before:bg-[var(--quote-accent,var(--accent))] max-[52rem]:flex-col max-[52rem]:items-stretch max-[52rem]:px-5",
  quoteHeroIdentity: "relative flex min-w-0 items-center gap-4",
  quoteHeroIcon:
    "inline-flex size-12 shrink-0 items-center justify-center rounded-xl bg-surface text-[var(--quote-accent,var(--accent))] shadow-sm ring-1 ring-default",
  quoteEyebrow:
    "mb-1 text-xs font-semibold uppercase tracking-[0.14em] text-[var(--quote-accent,var(--accent))]",
  quoteCustomer:
    "m-0 truncate text-2xl font-semibold tracking-tight text-[var(--quote-text,var(--text-primary))] max-[52rem]:text-xl",
  quotePreparedFor:
    "mb-0 mt-1 text-sm text-[var(--quote-text,var(--text-secondary))] opacity-75",
  quoteHeroMetrics:
    "relative grid min-w-[390px] grid-cols-2 divide-x divide-default overflow-hidden rounded-xl border border-default bg-surface/90 shadow-sm max-[52rem]:min-w-0 max-[36rem]:grid-cols-1 max-[36rem]:divide-x-0 max-[36rem]:divide-y",
  quoteMetric:
    "flex min-h-24 flex-col justify-center gap-1 px-5 py-4 [&>span]:text-xs [&>span]:font-semibold [&>span]:uppercase [&>span]:tracking-wide [&>span]:text-tertiary [&>strong]:text-lg [&>strong]:font-semibold [&>strong]:tabular-nums [&>strong]:text-primary [&>small]:text-xs [&>small]:text-tertiary",
  documentSummary:
    "overflow-hidden rounded-xl border border-default bg-surface shadow-sm",
  headerFields:
    "grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-x-8 gap-y-5 p-5",
  documentNote: "border-t border-subtle bg-raised/40 px-5 py-4",
  readOnlyValue:
    "m-0 min-h-5 py-1 text-[0.9375rem] font-medium leading-relaxed text-primary",
  createBar:
    "flex items-center gap-4 border-t border-subtle pt-2 [&>p]:mr-auto",
  lines: "flex flex-col gap-3",
  linesHead: "flex min-h-10 items-center justify-between gap-3",
  sectionTitle:
    "m-0 text-xs font-semibold uppercase tracking-wide text-tertiary",
  lineDescription: "flex min-w-[260px] flex-col gap-1",
  creditList:
    "flex flex-col gap-2 [&_li]:flex [&_li]:items-center [&_li]:gap-2",
  totals:
    "min-w-[min(360px,100%)] self-end rounded-xl border border-default bg-raised/40 p-5",
  totalsList: "m-0 flex flex-col gap-2",
  totalsRow:
    "flex justify-between gap-6 text-sm text-secondary [&_dd]:m-0 [&_dt]:m-0",
  totalsGross:
    "mt-2 border-t border-default pt-3 text-base font-semibold text-primary",
  totalsNote: "mt-2 text-xs text-tertiary",
  stale: "opacity-55",
  actionBar:
    "flex flex-wrap items-center justify-end gap-3 rounded-xl border border-default bg-raised/40 p-4",
  relation: "m-0 text-sm",
  documentFooter:
    "rounded-xl border border-default bg-surface p-5 shadow-sm [&>p+section]:mt-4",
} as const;

export default styles;
