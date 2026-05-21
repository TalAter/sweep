/**
 * Cross-module type barrel. Each module owns its types and exports them
 * locally; this file re-exports them so callers have a single import site.
 *
 * Per spec §`src/types.ts`.
 */
export type { FetchedScript } from "./installer/fetch.ts";
export type { InstallCommand, ParseError } from "./installer/parse.ts";
export type { Invocation, Outcome } from "./store/invocations.ts";
export type { PackageRow } from "./store/packages.ts";
