/** Poll `check` until it stops throwing; re-throw the last error at timeout. */
export async function waitFor(
  check: () => void,
  { timeout = 2000, interval = 10 }: { timeout?: number; interval?: number } = {},
): Promise<void> {
  const start = Date.now();
  let lastError: unknown;
  while (true) {
    try {
      check();
      return;
    } catch (e) {
      lastError = e;
    }
    if (Date.now() - start >= timeout) throw lastError;
    await new Promise((r) => setTimeout(r, interval));
  }
}
