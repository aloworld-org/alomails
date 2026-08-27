export function LineField({ label, children }: { label: string; children: React.ReactNode }) {
  return <label className="min-w-0"><span className="mb-2 block text-xs font-semibold uppercase tracking-wide text-tertiary">{label}</span>{children}</label>;
}
