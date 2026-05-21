import { commands } from "./registry.ts";

export async function dispatch(name: string, argv: string[]): Promise<number> {
  const cmd = commands.find((c) => c.name === name);
  // Unreachable in production: main.ts only calls dispatch on exact-match
  // verbs from the registry. Kept as a defensive guard.
  if (!cmd) throw new Error(`unknown verb: ${name}`);
  return cmd.run(argv);
}
