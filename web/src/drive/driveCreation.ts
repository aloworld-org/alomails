export function nextUntitledName(
  base: string,
  names: readonly string[],
): string {
  const cleanBase = base.trim();
  const existing = new Set(
    names.map((name) => name.trim().toLocaleLowerCase()),
  );

  if (!existing.has(cleanBase.toLocaleLowerCase())) return cleanBase;

  let suffix = 2;
  while (existing.has(`${cleanBase} ${suffix}`.toLocaleLowerCase()))
    suffix += 1;
  return `${cleanBase} ${suffix}`;
}
