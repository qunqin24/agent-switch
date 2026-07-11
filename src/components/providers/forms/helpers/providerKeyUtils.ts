export function normalizeProviderKey(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9-]/g, "");
}
