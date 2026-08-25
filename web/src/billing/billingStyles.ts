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
  page: "mx-auto flex min-h-0 w-full max-w-[112rem] flex-1 flex-col gap-5 overflow-hidden px-8 pb-8 pt-6 max-[52rem]:p-4",
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
    "inline-flex min-h-9 items-center gap-1.5 rounded-lg bg-transparent !px-3 !py-2 text-sm font-medium !text-secondary !no-underline transition-colors hover:bg-raised hover:!text-accent hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent",
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
    "w-full min-h-16 resize-y rounded-md border border-default bg-surface px-3 py-2 font-[inherit] text-base text-primary placeholder:text-tertiary transition-colors duration-[var(--duration-fast)] ease-standard focus-visible:border-strong focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-1",
  chips: "inline-flex flex-wrap items-center gap-2",
  overdueRow: "[&_td]:bg-[var(--danger-tint)]",
  notice:
    "m-0 rounded-lg border border-default bg-raised p-3 text-sm text-secondary",
  loading: "flex flex-1 items-center justify-center p-10",
  dataLoading:
    "flex min-h-48 flex-1 flex-col items-center justify-center gap-3 rounded-xl border border-subtle bg-surface text-sm text-secondary",
  editor: "overflow-auto",
  editorHead:
    "flex flex-wrap items-center gap-3 border-b border-subtle px-6 pb-4 pt-5",
  editorTitle: "m-0 text-lg font-semibold text-primary",
  saveState: "ml-auto whitespace-nowrap text-xs text-tertiary",
  editorBody: "flex flex-col gap-5 px-6 pb-8 pt-5",
  headerFields: "grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-4",
  readOnlyValue: "m-0 min-h-5 py-2 text-sm text-primary",
  createBar:
    "flex items-center gap-4 border-t border-subtle pt-2 [&>p]:mr-auto",
  lines: "flex flex-col gap-2",
  linesHead: "flex items-center justify-between gap-3",
  sectionTitle:
    "m-0 text-xs font-semibold uppercase tracking-wide text-tertiary",
  lineDescription: "flex min-w-[260px] flex-col gap-1",
  creditList:
    "flex flex-col gap-2 [&_li]:flex [&_li]:items-center [&_li]:gap-2",
  totals: "min-w-[min(320px,100%)] self-end",
  totalsList: "m-0 flex flex-col gap-1",
  totalsRow:
    "flex justify-between gap-6 text-sm text-secondary [&_dd]:m-0 [&_dt]:m-0",
  totalsGross: "mt-2 border-t border-default pt-2 font-semibold text-primary",
  totalsNote: "mt-2 text-xs text-tertiary",
  stale: "opacity-55",
  actionBar: "flex flex-wrap justify-end gap-2 border-t border-subtle pt-3",
  relation: "m-0 text-sm",
} as const;

export default styles;
