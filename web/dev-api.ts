export function resolveDevApi(configured: string | undefined): string {
  return configured?.trim() || "http://localhost:8080";
}
